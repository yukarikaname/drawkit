package dev.drawkit.android.core

import android.view.Surface
import kotlin.math.atan2
import kotlin.math.hypot

/**
 * The resident GPU surface. Layers, the live stroke and the tile journal live here; the CPU bitmap
 * is only rebuilt when something reads it.
 */
class GpuCanvas(private val editor: Long, val width: Int, val height: Int) {
    var handle = 0L
        private set
    private var layerCount = -1
    var stale = false
        private set

    val active: Boolean get() = handle != 0L

    fun attach(surface: Surface, viewWidth: Int, viewHeight: Int): Boolean {
        detach()
        handle = Native.canvasCreate(surface, viewWidth, viewHeight, false)
        if (handle == 0L) return false
        Native.canvasSetBudget(handle, 1536L shl 20)
        layerCount = -1
        return true
    }

    fun detach() {
        if (handle != 0L) Native.canvasFree(handle)
        handle = 0L
        layerCount = -1
        stale = false
    }

    fun resize(viewWidth: Int, viewHeight: Int) {
        if (handle != 0L) Native.canvasResize(handle, viewWidth, viewHeight)
    }

    fun needsLayers(count: Int) = handle != 0L && layerCount != count

    fun resetLayers(count: Int) {
        if (handle == 0L) return
        Native.canvasResetLayers(handle, count, width, height)
        layerCount = count
    }

    fun setLayer(index: Int, opacity: Float, blend: Int, visible: Boolean,
                 fill: FloatArray?, clip: Int) {
        if (handle == 0L) return
        Native.canvasSetLayer(handle, index, opacity, blend, visible, fill != null,
            fill?.get(0) ?: 0f, fill?.get(1) ?: 0f,
            fill?.get(2) ?: 0f, fill?.get(3) ?: 0f)
        Native.canvasSetLayerClip(handle, index, clip)
    }

    fun upload(index: Int, pixels: ByteArray) {
        if (handle != 0L) Native.canvasUploadLayer(handle, index, 0, 0, width, height, pixels)
    }

    fun present(paper: FloatArray) {
        if (handle != 0L) {
            Native.canvasPresent(handle, paper[0], paper[1], paper[2], paper[3])
        }
    }

    fun begin(color: FloatArray, flow: Float, pressureAlpha: Float, paper: Float,
              tip: Float, dual: FloatArray) {
        if (handle != 0L) {
            Native.canvasStrokeBegin(handle, width, height,
                color[0], color[1], color[2], color[3], flow, pressureAlpha, paper, tip,
                dual[0], dual[1], dual[2], dual[3], dual[4], dual[5], dual[6])
        }
    }

    fun segment(fromX: Float, fromY: Float, toX: Float, toY: Float, radius: Float,
                pressure: Float, tiltX: Float, tiltY: Float, azimuth: Float) {
        if (handle == 0L) return
        val pressureRadius = Native.canvasBrushParam(editor, 2)
        val painted = radius * (pressureRadius + (1f - pressureRadius) * pressure)
        val lean = hypot(tiltX, tiltY)
        val strength = (if (lean <= 1f) lean else lean / 60f).coerceIn(0f, 1f)
        val direction = if (lean > 0.01f) {
            Math.toDegrees(atan2(tiltY, tiltX).toDouble()).toFloat()
        } else {
            Math.toDegrees(azimuth.toDouble()).toFloat()
        }
        val roundness = maxOf(Native.canvasBrushParam(editor, 7) * (1f - 0.85f * strength), 0.08f)
        Native.canvasStrokeSegment(handle, fromX, fromY, toX, toY, painted,
            Native.canvasBrushParam(editor, 0), Native.canvasBrushParam(editor, 4),
            roundness, Native.canvasBrushParam(editor, 6) + direction, pressure)
    }

    fun commit(layer: Int, color: FloatArray, flow: Float): Int =
        if (handle == 0L) -1 else Native.canvasCommitStroke(editor, handle, layer,
            color[0], color[1], color[2], flow)

    fun endStroke() {
        if (handle != 0L) Native.canvasStrokeEnd(handle)
    }

    fun undoTiles() {
        if (handle != 0L) Native.canvasUndoTiles(handle)
        stale = true
    }

    fun redoTiles() {
        if (handle != 0L) Native.canvasRedoTiles(handle)
        stale = true
    }

    fun markStale() {
        stale = true
    }

    /** Rebuild the CPU bitmap for everything the GPU painted. */
    fun materialize() {
        if (handle != 0L && stale) {
            Native.canvasMaterialize(editor, handle)
            stale = false
        }
    }
}
