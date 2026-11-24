package dev.drawkit.android.core

import android.graphics.Bitmap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color

enum class Tool(val label: String) {
    Brush("Brush"),
    Eraser("Eraser"),
    Fill("Fill"),
}

/**
 * The window state: the tool, the colour, the layer list and the paint controller. Both the Compose
 * chrome and the drawing view read it, so a change from either side is immediately visible.
 */
class PaintState {
    val controller = PaintController()

    var tool by mutableStateOf(Tool.Brush)
    var brushSize by mutableFloatStateOf(18f)
    var opacity by mutableFloatStateOf(1f)
    var stabilization by mutableFloatStateOf(0f)
    /** 0 off, 1 horizontal, 2 vertical, 3 both. */
    var mirror by mutableIntStateOf(0)
    var autoShape by mutableStateOf(false)
    var tolerance by mutableFloatStateOf(0.12f)
    var fillClose by mutableFloatStateOf(1f)
    var fillExpand by mutableFloatStateOf(1f)
    var fillFeather by mutableFloatStateOf(0f)
    var color by mutableStateOf(Color(0.10f, 0.09f, 0.08f, 1f))
    var layers by mutableStateOf(emptyList<LayerRow>())
    var activeLayer by mutableIntStateOf(0)
    var canUndo by mutableStateOf(false)
    var canRedo by mutableStateOf(false)
    var isBlank by mutableStateOf(true)
    var revision by mutableIntStateOf(0)
    var showLayers by mutableStateOf(false)
    var showColours by mutableStateOf(false)
    var thumbnails by mutableStateOf<Map<Int, Bitmap>>(emptyMap())

    val recent = mutableStateListOf<Color>()
    var onInvalidate: (() -> Unit)? = null

    init {
        controller.setStabilization(stabilization)
        refreshLayers()
    }

    fun beginStroke() {
        controller.begin()
    }

    fun segment(
        fromX: Float, fromY: Float, toX: Float, toY: Float, pressure: Float,
        tiltX: Float, tiltY: Float, azimuth: Float, roll: Float, erase: Boolean,
    ) {
        controller.segment(
            fromX, fromY, toX, toY,
            brushSize / 2f, pressure,
            tiltX, tiltY, azimuth, roll,
            color.red, color.green, color.blue, color.alpha * opacity,
            erase,
        )
    }

    /** Called after a pointer-up or a fill. */
    fun finishStroke() {
        controller.end()
        val shaped = autoShape && controller.autoShape()
        if (shaped) controller.full() else controller.applyDirty()
        markPainted()
        rememberColour()
        refreshLayers()
    }

    fun chooseAutoShape(enabled: Boolean) {
        autoShape = enabled
        controller.setAutoShape(enabled)
    }

    /** Called after every segment so the bitmap and the cursor follow the pen. */
    fun afterSegment() {
        if (controller.applyDirty()) {
            isBlank = false
        }
        revision++
        onInvalidate?.invoke()
    }

    fun fill(x: Int, y: Int) {
        controller.fill(
            x, y, tolerance,
            fillClose.toInt(), fillExpand.toInt(), fillFeather.toInt(),
            color.copy(alpha = color.alpha * opacity),
        )
        controller.applyDirty()
        markPainted()
        rememberColour()
        refreshLayers()
    }

    fun undo() {
        if (!controller.undo()) return
        controller.full()
        refreshLayers()
        canUndo = controller.canUndo()
        canRedo = controller.canRedo()
        isBlank = controller.isClear()
        revision++
        onInvalidate?.invoke()
    }

    fun redo() {
        if (!controller.redo()) return
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

    private fun markPainted() {
        isBlank = false
        canUndo = controller.canUndo()
        canRedo = controller.canRedo()
        revision++
        onInvalidate?.invoke()
    }

    private fun layerChanged() {
        controller.full()
        refreshLayers()
        isBlank = controller.isClear()
        revision++
        onInvalidate?.invoke()
    }

    private fun refreshLayers() {
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
