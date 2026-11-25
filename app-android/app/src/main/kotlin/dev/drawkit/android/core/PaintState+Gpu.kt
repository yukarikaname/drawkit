package dev.drawkit.android.core

import android.view.Surface
import kotlin.math.pow

internal fun PaintState.strokeColour(): FloatArray {
    val gain = if (hdr) 2f.pow(glow) else 1f
    return floatArrayOf(
        color.red * gain, color.green * gain, color.blue * gain, color.alpha * opacity,
    )
}

/** Bind the drawing surface. False means the device took no swapchain. */
internal fun PaintState.attachSurface(
    surface: Surface, viewWidth: Int, viewHeight: Int,
): Boolean {
    if (hdr) return false
    val canvas = gpu ?: GpuCanvas(controller.editorHandle, controller.width, controller.height)
        .also { gpu = it }
    if (!canvas.attach(surface, viewWidth, viewHeight)) return false
    gpuLayersDirty = true
    syncGpu()
    return true
}

internal fun PaintState.detachSurface() {
    gpuStroke = false
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
            if (kind == 0) canvas.upload(index, controller.layerBytes(index))
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
    gpu?.present(paper)
}

internal fun PaintState.beginGpuStroke(): Boolean {
    val canvas = gpu
    val eligible = canvas != null && canvas.active && tool == Tool.Brush &&
        !hdr && !autoShape && !quickShape
    if (!eligible) return false
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
        Native.canvasBrushKindParam(kind, 5), Native.canvasBrushKindParam(kind, 8))
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
    controller.setDeferRaster(false)
    val canvas = gpu
    val count = canvas?.commit(controller.activeLayer(), strokeColour(),
        controller.brushParam(1)) ?: -1
    canvas?.endStroke()
    if (count < 0) {
        controller.flushDeferred()
        controller.applyDirty()
        gpuLayersDirty = true
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
