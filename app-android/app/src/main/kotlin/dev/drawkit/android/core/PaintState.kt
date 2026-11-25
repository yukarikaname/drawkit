package dev.drawkit.android.core

import android.graphics.Bitmap
import android.os.SystemClock
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
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

    val fillScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var fillJob: Job? = null

    var tool by mutableStateOf(Tool.Brush)
    var brushSize by mutableFloatStateOf(18f)
    var opacity by mutableFloatStateOf(1f)
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

    init {
        controller.setBrush(brushKind)
        controller.setQuickShape(quickShape)
        controller.setStabilization(stabilization)
        refreshLayers()
        refreshFrames()
    }

    fun chooseHdr(value: Boolean) {
        controller.setHdr(value)
        hdr = controller.hdr
        revision++
        onInvalidate?.invoke()
    }

    fun chooseBrush(kind: Int) {
        brushKind = kind
        controller.setBrush(kind)
    }

    fun beginStroke() {
        stopPlayback()
        controller.begin()
        if (!beginGpuStroke()) gpu?.materialize()
    }

    fun segment(
        fromX: Float, fromY: Float, toX: Float, toY: Float, pressure: Float,
        tiltX: Float, tiltY: Float, azimuth: Float, roll: Float, erase: Boolean,
    ) {
        val gain = if (hdr) 2f.pow(glow) else 1f
        controller.segment(
            fromX, fromY, toX, toY,
            brushSize / 2f, pressure,
            tiltX, tiltY, azimuth, roll,
            color.red * gain, color.green * gain, color.blue * gain, color.alpha * opacity,
            erase,
        )
        segmentGpu(fromX, fromY, toX, toY, pressure, tiltX, tiltY, azimuth)
    }

    /** Called after a pointer-up or a fill. */
    fun finishStroke(quickShapeRequested: Boolean = false) {
        controller.end()
        if (finishGpuStroke()) return
        val shaped = (autoShape || (quickShapeRequested && quickShape)) && controller.autoShape()
        if (shaped) controller.full() else controller.applyDirty()
        markPainted()
        rememberColour()
        refreshLayers()
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
        gpu?.materialize()
        val out = FloatArray(4)
        if (controller.pick(x, y, out) == 0) return false
        color = Color(out[0], out[1], out[2], out[3])
        rememberColour()
        return true
    }

    /** Start a fill that spreads from the tap. It becomes one undo step when the task ends. */
    fun beginFill(x: Int, y: Int): Boolean {
        finishFillAnimation()
        val ink = glowInk()
        val started = SystemClock.uptimeMillis()
        if (!controller.fillBegin(x, y, tolerance, fillClose.toInt(), fillExpand.toInt(),
                fillFeather.toInt(), ink, started)) {
            return false
        }
        isBlank = false
        fillJob = fillScope.launch {
            val start = SystemClock.uptimeMillis()
            while (isActive) {
                val progress = ((SystemClock.uptimeMillis() - start) / 220f).coerceIn(0f, 1f)
                val eased = 1f - (1f - progress).pow(3)
                controller.fillStep(eased)
                controller.applyDirty()
                revision++
                onInvalidate?.invoke()
                if (progress >= 1f) break
                delay(16)
            }
            controller.fillFinish()
            fillJob = null
            markPainted()
            rememberColour()
            refreshLayers()
        }
        return true
    }

    /// A HDR document lets the picked colour go above paper white; SDR leaves it alone.
    private fun glowInk(): Color {
        val gain = if (hdr) 2f.pow(glow) else 1f
        return Color(color.red * gain, color.green * gain, color.blue * gain,
                     color.alpha * opacity)
    }

    fun extractLineArt() {
        fillJob?.cancel()
        fillJob = null
        if (!controller.extractLineArt()) return
        controller.full()
        markPainted()
        refreshLayers()
    }

    fun finishFillAnimation() {
        gpu?.materialize()
        gpuLayersDirty = true
        if (fillJob == null) return
        fillJob?.cancel()
        fillJob = null
        controller.fillFinish()
        controller.applyDirty()
        markPainted()
        rememberColour()
        refreshLayers()
    }

    fun fill(x: Int, y: Int) {
        controller.fill(
            x, y, tolerance,
            fillClose.toInt(), fillExpand.toInt(), fillFeather.toInt(),
            glowInk(),
        )
        controller.applyDirty()
        markPainted()
        rememberColour()
        refreshLayers()
    }

    fun undo() {
        finishFillAnimation()
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
        finishFillAnimation()
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
        fillJob?.cancel()
        fillJob = null
        controller.fillCancel()
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
