package dev.drawkit.android.core

import android.view.Surface
import kotlin.math.pow

internal fun PaintState.strokeColour(): FloatArray {
    val gain = if (hdr) 2f.pow(glow) else 1f
    return floatArrayOf(
        color.red * gain, color.green * gain, color.blue * gain, color.alpha * activeOpacity,
    )
}

/** Bind the drawing surface. False means the device took no swapchain. */
internal fun PaintState.attachSurface(
    surface: Surface, viewWidth: Int, viewHeight: Int,
): Boolean {
    val canvas = gpu ?: GpuCanvas(controller.editorHandle, controller.width, controller.height)
        .also { gpu = it }
    windowSurface = surface
    windowSize = viewWidth to viewHeight
    if (!canvas.attach(surface, viewWidth, viewHeight, hdr)) return false
    // A driver that cannot present half floats degrades the swapchain, and the resident stack is
    // then eight-bit; keep the document in that range rather than upload floats into it.
    if (canvas.active && canvas.hdr != hdr) {
        controller.setHdr(canvas.hdr)
        this.hdr = controller.hdr
        if (!canvas.attach(surface, viewWidth, viewHeight, this.hdr)) return false
        if (this.hdr != canvas.hdr) report("This display cannot show extended range")
    }
    gpuLayersDirty = true
    syncGpu()
    return true
}

internal fun PaintState.detachSurface() {
    gpuStroke = false
    // The tiles are about to be freed with the swapchain; read back whatever the CPU cache has
    // not seen yet, or the next surface would show a document missing the last strokes.
    gpu?.materialize()
    gpu?.detach()
}

/** Upload what the resident stack is missing; cheap when nothing changed. */
private fun PaintState.syncGpu() {
    val canvas = gpu ?: return
    if (!canvas.active) return
    val count = controller.layerCount()
    var signature = 17
    for (index in 0 until count) {
        signature = signature * 31 + (controller.layerOpacity(index) * 1000).toInt()
        signature = signature * 31 + if (controller.layerVisible(index)) 1 else 0
        signature = signature * 31 + controller.layerKind(index)
    }
    if (canvas.needsLayers(count) || gpuLayersDirty) {
        canvas.materialize()
        canvas.resetLayers(count)
        for (index in 0 until count) {
            val kind = controller.layerKind(index)
            val fill = if (kind == 2) {
                FloatArray(4).also { controller.layerFill(index, it) }
            } else {
                null
            }
            canvas.setLayer(index, controller.layerOpacity(index), 0,
                controller.layerVisible(index), fill, -1)
            if (kind == 0) {
                if (canvas.hdr) {
                    canvas.uploadLinear(index, controller.layerLinear(index))
                } else {
                    canvas.upload(index, controller.layerBytes(index))
                }
            }
        }
        gpuLayersDirty = false
        gpuSignature = signature
        return
    }
    if (signature != gpuSignature) {
        for (index in 0 until count) {
            val kind = controller.layerKind(index)
            val fill = if (kind == 2) {
                FloatArray(4).also { controller.layerFill(index, it) }
            } else {
                null
            }
            canvas.setLayer(index, controller.layerOpacity(index), 0,
                controller.layerVisible(index), fill, -1)
        }
        gpuSignature = signature
    }
}

/** Draw the resident stack into the surface. `paper` is x, y, width, height in view pixels. */
internal fun PaintState.present(paper: FloatArray) {
    syncGpu()
    // One submit per frame: the segments queued since the last present land here.
    gpu?.flushStroke()
    gpu?.present(paper)
}

/** Whether the next stroke can be painted on the resident stack. */
internal val PaintState.gpuStrokeEligible: Boolean
    get() = gpu?.active == true && (tool == Tool.Brush || tool == Tool.Eraser)

internal fun PaintState.beginGpuStroke(): Boolean {
    val canvas = gpu
    if (!gpuStrokeEligible) return false
    val erase = tool == Tool.Eraser
    canvas ?: return false
    controller.setDeferRaster(true)
    val kind = controller.brushParam(14).toInt()
    val dual = floatArrayOf(
        controller.brushParam(12), controller.brushParam(13),
        Native.canvasBrushKindParam(kind, 0), Native.canvasBrushKindParam(kind, 7),
        Native.canvasBrushKindParam(kind, 6), Native.canvasBrushKindParam(kind, 9),
        Native.canvasBrushKindParam(kind, 10),
    )
    canvas!!.uploadBrushMap()
    canvas.begin(strokeColour(), controller.brushParam(1), controller.brushParam(3),
        controller.brushParam(9), controller.brushParam(10), dual,
        controller.brushParam(11), controller.brushParam(5), controller.brushParam(8),
        Native.canvasBrushKindParam(kind, 5), Native.canvasBrushKindParam(kind, 8),
        erase, controller.activeLayer())
    // Samples arrive faster than frames; queue them and let the present submit the batch.
    canvas.setStrokeBatch(true)
    gpuStroke = true
    return true
}

internal fun PaintState.segmentGpu(
    fromX: Float, fromY: Float, toX: Float, toY: Float, pressure: Float,
    tiltX: Float, tiltY: Float, azimuth: Float,
) {
    if (!gpuStroke) return
    gpu?.segment(fromX, fromY, toX, toY, brushSize / 2f, pressure,
        tiltX, tiltY, azimuth)
}

internal fun PaintState.afterGpuSegment(): Boolean {
    if (!gpuStroke) return false
    revision++
    onInvalidate?.invoke()
    return true
}

internal fun PaintState.finishGpuStroke(): Boolean {
    if (!gpuStroke) return false
    gpuStroke = false
    gpu?.setStrokeBatch(false)
    controller.setDeferRaster(false)
    val canvas = gpu
    val count = canvas?.commit(controller.activeLayer(), strokeColour(),
        controller.brushParam(1)) ?: -1
    canvas?.endStroke()
    if (count < 0) {
        // The surface refused the stroke. The tape keeps the samples, but nothing is painted on
        // the CPU: a cache the view does not show would only hide the failure.
        report("Stroke needs the GPU canvas")
    } else {
        canvas?.markStale()
    }
    markPainted()
    rememberColour()
    refreshLayers()
    revision++
    onInvalidate?.invoke()
    return true
}

/**
 * Replace the committed stroke with the ideal path the shape model recognised.
 *
 * The commit is swapped back in the GPU journal and the path is painted through a fresh resident
 * stroke: a snapped line stays on the tiles, and the CPU cache only sees it through the readback
 * that the commit already performs.
 */
internal fun PaintState.replayShape(snap: Boolean): Boolean {
    val path = controller.shapePath(snap) ?: return false
    if (!controller.undoIsGpu() || !controller.undo()) return false
    gpu?.undoTiles()
    controller.begin()
    if (!beginGpuStroke()) return false
    var lastX = path[0]
    var lastY = path[1]
    for (step in 1 until path.size / 2) {
        val x = path[step * 2]
        val y = path[step * 2 + 1]
        if (x == lastX && y == lastY) continue
        segment(lastX, lastY, x, y, 1f, 0f, 0f, 0f, 0f, false)
        lastX = x
        lastY = y
    }
    controller.end()
    finishGpuStroke()
    // The undone stroke left tiles the shape does not cover; read them back so the bitmap cache
    // stops showing the old line.
    gpu?.materialize()
    return true
}

internal fun PaintState.undoGpu(): Boolean {
    if (!controller.undoIsGpu()) return false
    if (!controller.undo()) return true
    gpu?.undoTiles()
    refreshLayers()
    canUndo = controller.canUndo()
    canRedo = controller.canRedo()
    revision++
    onInvalidate?.invoke()
    return true
}

internal fun PaintState.redoGpu(): Boolean {
    if (!controller.redoIsGpu()) return false
    if (!controller.redo()) return true
    gpu?.redoTiles()
    refreshLayers()
    canUndo = controller.canUndo()
    canRedo = controller.canRedo()
    revision++
    onInvalidate?.invoke()
    return true
}
