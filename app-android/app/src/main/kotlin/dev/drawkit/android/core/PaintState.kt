package dev.drawkit.android.core

import android.graphics.Bitmap
import android.view.Surface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlin.math.pow

enum class Tool(val label: String) {
    Brush("Brush"),
    Eraser("Eraser"),
    Fill("Fill"),
    Eyedropper("Eyedropper"),
    Select("Select"),
}

data class SelectionRect(val x: Int, val y: Int, val width: Int, val height: Int)

/**
 * The window state: the tool, the colour, the layer list and the paint controller. Both the Compose
 * chrome and the drawing view read it, so a change from either side is immediately visible.
 */
class PaintState {
    val controller = PaintController()

    /** The resident GPU surface, once the view has a window to draw into. */
    var gpu: GpuCanvas? = null
        internal set
    internal var gpuStroke = false
    internal var gpuLayersDirty = true
    internal var gpuSignature = 0
    /** The window surface the resident stack draws into, so a range change can rebuild it. */
    internal var windowSurface: Surface? = null
    internal var windowSize = 0 to 0

    /** One scope for playback; a fill no longer animates, it is written in one pass. */
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    var tool by mutableStateOf(Tool.Brush)
    var brushSize by mutableFloatStateOf(18f)
    var opacity by mutableFloatStateOf(1f)
    /** The eraser keeps its own size and strength, as the macOS workspace does. */
    var eraserSize by mutableFloatStateOf(24f)
    var eraserOpacity by mutableFloatStateOf(1f)
    var brushKind by mutableIntStateOf(0)
    var stabilization by mutableFloatStateOf(0f)
    /** 0 off, 1 horizontal, 2 vertical, 3 both. */
    var mirror by mutableIntStateOf(0)
    var autoShape by mutableStateOf(false)
    var quickShape by mutableStateOf(true)
    var selection by mutableStateOf<SelectionRect?>(null)
    var tolerance by mutableFloatStateOf(0.12f)
    var fillClose by mutableFloatStateOf(1f)
    var fillExpand by mutableFloatStateOf(1f)
    var fillFeather by mutableFloatStateOf(0f)
    var fillSnap by mutableStateOf(true)
    /** The mask buffer is canvas-sized and reused; a fill must not allocate it again. */
    internal var fillMaskBuffer: FloatArray? = null
    /** Whether the chrome follows Material You; the canvas never does. */
    var chromeFollowsSystem by mutableStateOf(true)
    var color by mutableStateOf(Color(0.10f, 0.09f, 0.08f, 1f))
    /** Stops above paper white for a new stroke in an HDR document. */
    var glow by mutableFloatStateOf(0f)
    var hdr by mutableStateOf(controller.hdr)
    var layers by mutableStateOf(emptyList<LayerRow>())
    var activeLayer by mutableIntStateOf(0)
    var canUndo by mutableStateOf(false)
    var canRedo by mutableStateOf(false)
    var isBlank by mutableStateOf(true)
    var revision by mutableIntStateOf(0)
    var showLayers by mutableStateOf(false)
    var showColours by mutableStateOf(false)
    var showAnimation by mutableStateOf(false)
    var thumbnails by mutableStateOf<Map<Int, Bitmap>>(emptyMap())
    var frameCount by mutableIntStateOf(1)
    var activeFrame by mutableIntStateOf(0)
    var frameThumbnails by mutableStateOf<List<Bitmap?>>(emptyList())
    var onionBitmap by mutableStateOf<Bitmap?>(null)
    var isPlaying by mutableStateOf(false)
    var fps by mutableIntStateOf(12)
    var frameHold by mutableIntStateOf(83)
    var loopEnabled by mutableStateOf(true)
    var loopStart by mutableIntStateOf(0)
    var loopEnd by mutableIntStateOf(1)
    var onionEnabled by mutableStateOf(true)
    var onionBefore by mutableIntStateOf(1)
    var onionAfter by mutableIntStateOf(0)
    var playbackJob: Job? = null

    val recent = mutableStateListOf<Color>()
    var onInvalidate: (() -> Unit)? = null
    var notice by mutableStateOf<String?>(null)
        private set
    var noticeId by mutableIntStateOf(0)
        private set

    /** The size and opacity the selected tool actually paints with. */
    val activeSize: Float get() = if (tool == Tool.Eraser) eraserSize else brushSize
    val activeOpacity: Float get() = if (tool == Tool.Eraser) eraserOpacity else opacity

    init {
        controller.setBrush(brushKind)
        controller.setQuickShape(quickShape)
        controller.setStabilization(stabilization)
        controller.setFillSnap(fillSnap)
        controller.setEraserStrength(eraserOpacity)
        refreshLayers()
        refreshFrames()
    }

    /**
     * Switch the document between standard and extended range. The swapchain format is fixed when
     * it is created, so a bound surface is rebuilt: the CPU cache is brought up to date first, the
     * resident tiles are dropped, and the new range is attached. Painting never runs on the CPU.
     */
    fun chooseHdr(value: Boolean) {
        if (value == hdr) return
        val surface = windowSurface
        val size = windowSize
        if (surface != null) {
            gpu?.materialize()
            detachSurface()
        }
        controller.setHdr(value)
        hdr = controller.hdr
        if (surface != null && !attachSurface(surface, size.first, size.second)) {
            report("The GPU canvas did not take the surface")
        }
        gpuLayersDirty = true
        revision++
        onInvalidate?.invoke()
    }

    fun chooseBrush(kind: Int) {
        brushKind = kind
        controller.setBrush(kind)
    }

    fun chooseFillSnap(enabled: Boolean) {
        fillSnap = enabled
        controller.setFillSnap(enabled)
    }

    fun chooseEraserOpacity(value: Float) {
        eraserOpacity = value
        controller.setEraserStrength(value)
    }

    fun chooseChrome(followSystem: Boolean) {
        chromeFollowsSystem = followSystem
    }

    /** The macOS equivalent flashes a message over the canvas; a toast is the native form. */
    internal fun report(text: String) {
        notice = text
        noticeId++
    }

    /**
     * Start a stroke on the resident stack. There is no CPU stroke path: a surface that failed to
     * attach is reported, because pixels painted into the bitmap would never reach the tiles the
     * view shows.
     */
    fun beginStroke() {
        stopPlayback()
        // The core decides at `begin` whether a session records or rasterises, so the flag has to
        // be set first; setting it afterwards left every Android stroke rasterising on the CPU.
        if (gpuStrokeEligible) controller.setDeferRaster(true)
        controller.begin()
        if (!beginGpuStroke()) {
            strokeRefused = true
            controller.end()
            report("Drawing needs the GPU canvas")
        }
    }

    fun segment(
        fromX: Float, fromY: Float, toX: Float, toY: Float, pressure: Float,
        tiltX: Float, tiltY: Float, azimuth: Float, roll: Float, erase: Boolean,
    ) {
        if (strokeRefused) return
        val gain = if (hdr) 2f.pow(glow) else 1f
        controller.segment(
            fromX, fromY, toX, toY,
            activeSize / 2f, pressure,
            tiltX, tiltY, azimuth, roll,
            color.red * gain, color.green * gain, color.blue * gain,
            color.alpha * activeOpacity,
            erase,
        )
        segmentGpu(fromX, fromY, toX, toY, pressure, tiltX, tiltY, azimuth)
    }

    /** Called after a pointer-up or a fill. */
    fun finishStroke(quickShapeRequested: Boolean = false) {
        if (strokeRefused) {
            strokeRefused = false
            return
        }
        controller.end()
        if (!gpuStroke) {
            finishGpuStroke()
            return
        }
        finishGpuStroke()
        if (autoShape || (quickShapeRequested && quickShape)) {
            replayShape(snap = false)
        }
    }

    fun chooseSelection(rect: SelectionRect?) {
        if (rect == null || rect.width < 2 || rect.height < 2) {
            controller.clearSelection()
            selection = null
        } else {
            controller.setSelection(rect.x, rect.y, rect.width, rect.height)
            selection = rect
        }
        revision++
    }

    fun extractSelection(): Boolean {
        gpu?.materialize()
        gpuLayersDirty = true
        val changed = controller.extractSelection()
        if (changed) {
            controller.full()
            refreshLayers()
            revision++
            onInvalidate?.invoke()
        }
        return changed
    }

    fun chooseAutoShape(enabled: Boolean) {
        autoShape = enabled
        controller.setAutoShape(enabled)
    }

    internal var strokeRefused = false

    fun chooseQuickShape(enabled: Boolean) {
        quickShape = enabled
        controller.setQuickShape(enabled)
    }

    /** Called after every segment so the bitmap and the cursor follow the pen. */
    fun afterSegment() {
        if (afterGpuSegment()) return
        if (controller.applyDirty()) {
            isBlank = false
        }
        revision++
        onInvalidate?.invoke()
    }

    fun pick(x: Int, y: Int): Boolean {
        val canvas = gpu
        if (canvas == null || !canvas.active) {
            report("Colour pick needs the GPU canvas")
            return false
        }
        val pixel = canvas.pick(x, y) ?: return false
        color = Color(pixel[0], pixel[1], pixel[2], pixel[3])
        rememberColour()
        return true
    }

    fun extractLineArt() {
        if (!controller.extractLineArt()) return
        controller.full()
        gpuLayersDirty = true
        markPainted()
        refreshLayers()
    }

    fun undo() {
        if (undoGpu()) return
        gpu?.materialize()
        if (!controller.undo()) return
        gpuLayersDirty = true
        controller.full()
        refreshLayers()
        canUndo = controller.canUndo()
        canRedo = controller.canRedo()
        isBlank = controller.isClear()
        revision++
        onInvalidate?.invoke()
    }

    fun redo() {
        if (redoGpu()) return
        gpu?.materialize()
        if (!controller.redo()) return
        gpuLayersDirty = true
        controller.full()
        refreshLayers()
        canUndo = controller.canUndo()
        canRedo = controller.canRedo()
        isBlank = controller.isClear()
        revision++
        onInvalidate?.invoke()
    }

    fun addLayer() {
        controller.addLayer()
        layerChanged()
    }

    fun addGroup() {
        controller.addGroup()
        layerChanged()
    }

    fun addFillLayer() {
        controller.addFill(color, color.alpha * opacity)
        layerChanged()
    }

    fun setExpanded(index: Int, expanded: Boolean) {
        controller.setExpanded(index, expanded)
        refreshLayers()
        revision++
    }

    fun chooseMirror(mode: Int) {
        mirror = mode
        controller.setMirror(mode)
        revision++
        onInvalidate?.invoke()
    }

    /** Rows in sheet order: top of the stack first, children under an open group. */
    fun layerRows(): List<LayerRow> {
        val rows = mutableListOf<LayerRow>()
        fun add(parent: Int?) {
            layers.filter { it.parent == parent }
                .sortedByDescending { it.index }
                .forEach { layer ->
                    rows.add(layer)
                    if (layer.isGroup && layer.expanded) add(layer.index)
                }
        }
        add(null)
        return rows
    }

    fun moveLayer(index: Int, steps: Int) {
        if (controller.moveLayer(index, steps) != 0) layerChanged()
    }

    fun removeLayer() {
        if (controller.removeLayer() != 0) layerChanged()
    }

    fun selectLayer(index: Int) {
        controller.selectLayer(index)
        activeLayer = controller.activeLayer()
        revision++
        onInvalidate?.invoke()
    }

    fun setVisible(index: Int, visible: Boolean) {
        controller.setVisible(index, visible)
        layerChanged()
    }

    fun chooseReference(index: Int, reference: Boolean) {
        controller.setReference(index, reference)
        refreshLayers()
        revision++
    }

    fun chooseTag(index: Int, tag: Int) {
        controller.setTag(index, tag)
        refreshLayers()
        revision++
    }

    fun renameLayer(index: Int, name: String) {
        controller.renameLayer(index, name)
        refreshLayers()
    }

    fun setOpacity(index: Int, value: Float) {
        controller.setOpacity(index, value)
        refreshLayers()
        revision++
        onInvalidate?.invoke()
    }

    fun clearLayer() {
        controller.clear(BrandPaper)
        controller.full()
        layerChanged()
    }

    fun toggleLayers() {
        showLayers = !showLayers
        if (showLayers) refreshThumbnails()
    }

    fun toggleColours() {
        showColours = !showColours
    }

    fun refreshThumbnails() {
        gpu?.materialize()
        thumbnails = layers.associate { layer -> layer.index to controller.thumbnail(layer.index) }
            .filterValues { it != null }
            .mapValues { it.value!! }
    }

    fun rememberColour() {
        val colour = color
        recent.removeAll { it == colour }
        recent.add(0, colour)
        while (recent.size > 10) recent.removeAt(recent.size - 1)
    }

    fun sampleArtwork() {
        val sampled = controller.palette(6)
        if (sampled.isEmpty()) return
        sampled.forEach { rgba ->
            val colour = Color(rgba[0], rgba[1], rgba[2], 1f)
            recent.removeAll { it == colour }
            recent.add(0, colour)
        }
        while (recent.size > 10) recent.removeAt(recent.size - 1)
        color = Color(sampled[0][0], sampled[0][1], sampled[0][2], 1f)
    }

    fun updateStabilization(value: Float) {
        stabilization = value
        controller.setStabilization(value)
    }

    fun release() {
        controller.release()
    }

    internal fun markPainted() {
        isBlank = false
        canUndo = controller.canUndo()
        canRedo = controller.canRedo()
        revision++
        onInvalidate?.invoke()
    }

    private fun layerChanged() {
        gpu?.materialize()
        gpuLayersDirty = true
        controller.full()
        refreshLayers()
        isBlank = controller.isClear()
        revision++
        onInvalidate?.invoke()
    }

    fun refreshLayers() {
        layers = controller.layers()
        activeLayer = controller.activeLayer()
        canUndo = controller.canUndo()
        canRedo = controller.canRedo()
    }

    companion object {
        val BrandPaper = Color(0.965f, 0.945f, 0.914f, 1f)
    }
}

/** HSL for the colour grid, matching the macOS panel so both pickers feel the same. */
fun hslToColor(hue: Float, saturation: Float, lightness: Float): Color {
    val c = (1f - kotlin.math.abs(2f * lightness - 1f)) * saturation
    val h = (hue - kotlin.math.floor(hue)) * 6f
    val x = c * (1f - kotlin.math.abs(h % 2f - 1f))
    val (r, g, b) = when (h.toInt()) {
        0 -> Triple(c, x, 0f)
        1 -> Triple(x, c, 0f)
        2 -> Triple(0f, c, x)
        3 -> Triple(0f, x, c)
        4 -> Triple(x, 0f, c)
        else -> Triple(c, 0f, x)
    }
    val m = lightness - c / 2f
    return Color(r + m, g + m, b + m, 1f)
}
