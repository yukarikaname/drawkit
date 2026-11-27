package dev.drawkit.android.core

import androidx.compose.ui.graphics.Color
import kotlin.math.max
import kotlin.math.pow

/**
 * The resident fill: the surface exports the composite, the core builds the mask and the surface
 * writes it into the layer in one pass. There is no CPU fallback because the bitmap is only a
 * stale copy of the tiles; a fill that cannot reach the tiles is reported, not painted elsewhere.
 */
internal fun PaintState.beginFill(x: Int, y: Int): Boolean {
    val canvas = gpu
    if (hdr || canvas == null || !canvas.active) {
        reportFillUnavailable()
        return false
    }
    val width = controller.width
    val height = controller.height
    if (x !in 0 until width || y !in 0 until height) return false
    // The flood reads the composed frame, and the frame is on the GPU. Reading it back is the
    // same premultiplied RGBA8 frame a tape replay floods over, so both agree.
    val frame = canvas.exportPixels()
    if (frame == null) {
        reportFillUnavailable()
        return false
    }
    val layer = controller.activeLayer()
    // The fill goes under the line when the layer is empty at the tap; only the tile knows.
    val layerAlpha = canvas.layerAlpha(layer, x, y)
    val under = layerAlpha >= 0f && layerAlpha < 0.5f
    val at = (y * width + x) * 4
    val seed = floatArrayOf(
        (frame[at].toInt() and 0xFF) / 255f,
        (frame[at + 1].toInt() and 0xFF) / 255f,
        (frame[at + 2].toInt() and 0xFF) / 255f,
        max(layerAlpha, 0f),
    )
    val mask = fillMask(width * height)
    val rect = IntArray(7)
    val colour = FloatArray(4)
    val begun = controller.fillGpuBeginFrom(
        frame, width, height, x, y, tolerance,
        fillClose.toInt(), fillExpand.toInt(), fillFeather.toInt(),
        inkColour(), seed, under, controller.nextTime(), mask, rect, colour,
    )
    if (!begun) {
        reportFillUnavailable()
        return false
    }
    val dirty = IntArray(4)
    if (!canvas.applyFill(rect[0], mask, colour, rect[6] != 0, rect[5] != 0, dirty)) {
        // The mask exists but no tile changed; there is nothing to fall back to.
        controller.fillGpuCancel()
        reportFillUnavailable()
        return false
    }
    // The tiles advanced; the bitmap behind them has not. The next materialize reads them back.
    controller.fillGpuFinish(dirty, 1)
    canvas.markStale()
    markPainted()
    rememberColour()
    refreshLayers()
    return true
}

/** The canvas-sized mask, kept between fills: one allocation per document, not per tap. */
private fun PaintState.fillMask(size: Int): FloatArray {
    val existing = fillMaskBuffer
    if (existing != null && existing.size == size) return existing
    return FloatArray(size).also { fillMaskBuffer = it }
}

/** The ink the fill will blend, with the HDR glow a stroke would get. */
private fun PaintState.inkColour(): Color {
    val gain = if (hdr) 2f.pow(glow) else 1f
    return Color(
        color.red * gain, color.green * gain, color.blue * gain, color.alpha * opacity,
    )
}

private fun PaintState.reportFillUnavailable() {
    report("Fill needs the GPU canvas")
}

/** Composite readback at canvas size, premultiplied RGBA8. */
internal fun GpuCanvas.exportPixels(): ByteArray? {
    if (handle == 0L) return null
    val bytes = ByteArray(width * height * 4)
    return if (Native.canvasExportGpu(handle, width, height, bytes) != 0) bytes else null
}

/** One resident layer pixel's alpha, or a negative value when the tile cannot answer. */
internal fun GpuCanvas.layerAlpha(index: Int, x: Int, y: Int): Float =
    if (handle == 0L) -1f else Native.canvasLayerAlpha(handle, index, x, y)

/** Blend the prepared mask into the resident tiles; `dirty` receives the touched rectangle. */
internal fun GpuCanvas.applyFill(
    layer: Int, mask: FloatArray, colour: FloatArray, alphaLocked: Boolean, under: Boolean,
    dirty: IntArray,
): Boolean {
    if (handle == 0L) return false
    return Native.canvasApplyFill(
        handle, layer, mask, width, height,
        colour[0], colour[1], colour[2], colour[3], alphaLocked, under, dirty,
    ) != 0
}
