package dev.drawkit.android.core

import android.graphics.Bitmap
import android.graphics.ColorSpace
import android.os.SystemClock
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** One document, one handle and one bitmap. The bitmap is the only copy the UI draws. */
class PaintController(val width: Int = 1024, val height: Int = 1024) {
    private val handle: Long = Native.newEditor(width, height)
    var bitmap: Bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        private set
    var hdr = false
        private set
    private var half: ByteBuffer? = null

    private var scratch = IntArray(0)
    private val dirty = IntArray(4)
    private var time = SystemClock.uptimeMillis()

    init {
        hdr = Native.dynamicRange(handle) != 0
        allocateBitmap()
        full()
    }

    private fun allocateBitmap() {
        if (hdr) {
            bitmap = Bitmap.createBitmap(
                width, height, Bitmap.Config.RGBA_F16, true,
                ColorSpace.get(ColorSpace.Named.LINEAR_EXTENDED_SRGB),
            )
            half = ByteBuffer.allocateDirect(width * height * 8).order(ByteOrder.nativeOrder())
        } else {
            bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            half = null
        }
    }

    /** Switch the document to extended range. The bitmap becomes a float buffer; nothing is
     *  tone mapped, so values above paper white stay above one. */
    fun setHdr(value: Boolean) {
        if (value == hdr) return
        Native.setDynamicRange(handle, if (value) 1 else 0)
        hdr = value
        allocateBitmap()
        full()
    }

    val editorHandle: Long get() = handle

    /** Keep the stroke off the CPU while the resident surface paints it. */
    fun setDeferRaster(enabled: Boolean) = Native.canvasSetDeferRaster(handle, enabled)
    fun flushDeferred() = Native.canvasFlushDeferred(handle)
    fun brushParam(id: Int): Float = Native.canvasBrushParam(handle, id)
    fun undoIsGpu(): Boolean = Native.undoIsGpu(handle) != 0
    fun redoIsGpu(): Boolean = Native.redoIsGpu(handle) != 0

    /** One layer's pixels as straight RGBA8, for the first upload to the resident stack. */
    fun layerBytes(index: Int): ByteArray {
        val bytes = ByteArray(width * height * 4)
        Native.layerRect(handle, index, 0, 0, width, height, bytes)
        return bytes
    }

    fun layerFill(index: Int, out: FloatArray): Boolean = Native.layerFill(handle, index, out) != 0

    /** One layer as premultiplied linear Display P3 floats, for a half-float upload. */
    fun layerLinear(index: Int): FloatArray {
        val out = FloatArray(width * height * 4)
        Native.layerLinear(handle, index, out)
        return out
    }

    /**
     * The ideal line, oval or rectangle the last stroke snaps to, as `x, y` pairs. Null keeps the
     * freehand stroke. The shell replays the path through the resident surface.
     */
    fun shapePath(snap: Boolean): FloatArray? {
        val out = FloatArray(4096)
        val count = Native.shapePath(handle, snap, out)
        return if (count >= 2) out.copyOf(count * 2) else null
    }

    /** Release the native document; the bitmap stays valid but stops changing. */
    fun release() {
        Native.free(handle)
    }

    /** Recomposite the whole document. Used on first paint and after history or layer changes. */
    fun full() {
        if (hdr) {
            compositeHalf(0, 0, width, height)
            return
        }
        ensure(width * height)
        if (Native.compositeRect(handle, 0, 0, width, height, scratch) != 0) {
            bitmap.setPixels(scratch, 0, width, 0, 0, width, height)
        }
    }

    private fun compositeHalf(x: Int, y: Int, w: Int, h: Int) {
        val buffer = half ?: return
        buffer.clear()
        if (Native.compositeLinearHalfRect(handle, x, y, w, h, buffer) != 0) {
            buffer.position(0)
            buffer.limit(w * h * 8)
            bitmap.copyPixelsFromBuffer(buffer)
        }
    }

    /** Upload only the rectangle the last operation touched. Returns whether it painted. */
    fun applyDirty(): Boolean {
        if (Native.takeDirty(handle, dirty) == 0) return false
        val x = dirty[0]
        val y = dirty[1]
        val w = dirty[2]
        val h = dirty[3]
        if (w <= 0 || h <= 0) return false
        if (hdr) {
            // The float bitmap has no sub-rectangle upload; composite the whole document so the
            // reused half buffer always matches the destination bitmap.
            full()
            return true
        }
        ensure(w * h)
        if (Native.compositeRect(handle, x, y, w, h, scratch) != 0) {
            bitmap.setPixels(scratch, 0, w, x, y, w, h)
            return true
        }
        return false
    }

    fun begin() {
        time = SystemClock.uptimeMillis()
        Native.strokeBegin(handle, time)
    }

    fun segment(
        fromX: Float, fromY: Float, toX: Float, toY: Float,
        radius: Float, pressure: Float,
        tiltX: Float, tiltY: Float, azimuth: Float, roll: Float,
        red: Float, green: Float, blue: Float, alpha: Float,
        erase: Boolean,
    ) {
        // Tablet samples can share a millisecond; the core records the tape by time, so make each
        // segment strictly later than the one before it.
        time = maxOf(time + 1, SystemClock.uptimeMillis())
        if (erase) {
            Native.eraseEx(
                handle, fromX, fromY, toX, toY, radius, pressure,
                tiltX, tiltY, azimuth, roll, time,
            )
        } else {
            Native.strokeEx(
                handle, fromX, fromY, toX, toY, radius, pressure,
                tiltX, tiltY, azimuth, roll,
                red, green, blue, alpha, time,
            )
        }
    }

    fun end() {
        Native.strokeEnd(handle)
    }

    /** A strictly later tape time; tablet samples can share a millisecond. */
    internal fun nextTime(): Long {
        time = maxOf(time + 1, SystemClock.uptimeMillis())
        return time
    }

    /** Build the resident fill's mask from the composite the surface exported. */
    fun fillGpuBeginFrom(
        composite: ByteArray, width: Int, height: Int, x: Int, y: Int,
        tolerance: Float, close: Int, expand: Int, feather: Int,
        color: androidx.compose.ui.graphics.Color,
        seed: FloatArray, under: Boolean, timeMs: Long,
        mask: FloatArray, rect: IntArray, colour: FloatArray,
    ): Boolean = Native.fillGpuBeginFrom(
        handle, composite, width, height, x, y, tolerance, close, expand, feather,
        color.red, color.green, color.blue, color.alpha,
        seed[0], seed[1], seed[2], seed[3], under, timeMs, mask, rect, colour,
    )

    fun fillGpuFinish(rects: IntArray, count: Int): Boolean =
        Native.fillGpuFinish(handle, rects, count)

    fun fillGpuCancel(): Boolean = Native.fillGpuCancel(handle)
    fun setFillSnap(enabled: Boolean) = Native.setFillSnap(handle, enabled)
    fun setEraserStrength(strength: Float) = Native.setEraserStrength(handle, strength)
    fun extractLineArt(): Boolean = Native.extractLineArt(handle, 0.55f, 0.12f)

    fun setBrush(kind: Int) {
        Native.setBrush(handle, kind)
    }

    fun setQuickShape(enabled: Boolean) {
        Native.setQuickShape(handle, enabled)
    }

    fun setStabilization(strength: Float) {
        Native.setStabilization(handle, strength)
    }

    fun undo(): Boolean = Native.undo(handle) != 0
    fun redo(): Boolean = Native.redo(handle) != 0
    fun canUndo(): Boolean = Native.canUndo(handle)
    fun canRedo(): Boolean = Native.canRedo(handle)
    fun isClear(): Boolean = Native.isClear(handle)
    fun pick(x: Int, y: Int, out: FloatArray): Int = Native.pick(handle, x, y, out)

    fun clear(color: androidx.compose.ui.graphics.Color) {
        Native.clear(handle, color.red, color.green, color.blue, color.alpha)
    }

    fun layers(): List<LayerRow> {
        val count = Native.layerCount(handle)
        val parents = (0 until count).map { Native.layerParent(handle, it) }
        return (0 until count).map { index ->
            var depth = 0
            var parent = parents[index]
            while (parent >= 0 && parent < count) {
                depth++
                parent = parents[parent]
            }
            LayerRow(
                index = index,
                name = Native.layerName(handle, index) ?: "Layer ${index + 1}",
                visible = Native.layerVisible(handle, index),
                opacity = Native.layerOpacity(handle, index),
                kind = Native.layerKind(handle, index),
                parent = parents[index].takeIf { it >= 0 },
                expanded = Native.layerExpanded(handle, index),
                depth = depth,
                tag = Native.layerTag(handle, index),
                reference = Native.layerReference(handle, index),
            )
        }
    }

    fun activeLayer(): Int = Native.layerActive(handle)

    fun layerCount(): Int = Native.layerCount(handle)
    fun layerKind(index: Int): Int = Native.layerKind(handle, index)
    fun layerVisible(index: Int): Boolean = Native.layerVisible(handle, index)
    fun layerOpacity(index: Int): Float = Native.layerOpacity(handle, index)
    fun selectLayer(index: Int) = Native.layerSelect(handle, index)
    fun addLayer(): Int = Native.layerAdd(handle)
    fun addGroup(): Int = Native.layerAddGroup(handle)
    fun addFill(color: androidx.compose.ui.graphics.Color, alpha: Float): Int =
        Native.layerAddFill(handle, color.red, color.green, color.blue, alpha)
    fun setExpanded(index: Int, expanded: Boolean) =
        Native.layerSetExpanded(handle, index, expanded)
    fun setMirror(mode: Int) = Native.setMirror(handle, mode)
    fun setAutoShape(enabled: Boolean) = Native.setAutoShape(handle, enabled)
    fun autoShape(): Boolean = Native.autoShape(handle)
    fun setSelection(x: Int, y: Int, width: Int, height: Int) =
        Native.setSelection(handle, x, y, width, height)
    fun clearSelection() = Native.clearSelection(handle)
    fun extractSelection(): Boolean = Native.extractSelection(handle)
    fun moveLayer(index: Int, steps: Int): Int = Native.layerMove(handle, index, steps)
    fun removeLayer(): Int = Native.layerRemove(handle)
    fun setVisible(index: Int, visible: Boolean) = Native.layerSetVisible(handle, index, visible)
    fun setOpacity(index: Int, opacity: Float) = Native.layerSetOpacity(handle, index, opacity)
    fun setTag(index: Int, tag: Int) = Native.layerSetTag(handle, index, tag)
    fun setReference(index: Int, reference: Boolean) = Native.layerSetReference(handle, index, reference)
    fun renameLayer(index: Int, name: String) = Native.layerSetName(handle, index, name)

    fun thumbnail(index: Int, size: Int = 52): Bitmap? {
        val values = IntArray(size * size)
        if (Native.layerThumbnail(handle, index, size, size, values) == 0) return null
        return Bitmap.createBitmap(values, size, size, Bitmap.Config.ARGB_8888)
    }

    fun exportExr(path: String): Boolean = Native.exportExr(handle, path)
    fun exportBrush(path: String): Boolean = Native.exportBrush(handle, path)
    fun importBrush(path: String): Int = Native.importBrush(handle, path)

    fun frameCount(): Int = Native.frameCount(handle)
    fun frameActive(): Int = Native.frameActive(handle)
    fun frameAdd(duplicate: Boolean): Int = Native.frameAdd(handle, duplicate)
    fun frameSelect(index: Int): Boolean = Native.frameSelect(handle, index)
    fun frameRemove(index: Int): Boolean = Native.frameRemove(handle, index)
    fun fps(): Int = Native.fps(handle)
    fun setFps(value: Int) = Native.setFps(handle, value)
    fun frameDuration(): Int = Native.frameDuration(handle)
    fun setFrameDuration(value: Int) = Native.setFrameDuration(handle, value)
    fun loopEnabled(): Boolean = Native.loopEnabled(handle)
    fun setLoopEnabled(enabled: Boolean) = Native.setLoopEnabled(handle, enabled)
    fun loopStart(): Int = Native.loopStart(handle)
    fun setLoopStart(index: Int) = Native.setLoopStart(handle, index)
    fun loopEnd(): Int = Native.loopEnd(handle)
    fun setLoopEnd(end: Int) = Native.setLoopEnd(handle, end)

    fun frameThumbnail(index: Int, size: Int = 64): Bitmap? {
        val values = IntArray(width * height)
        if (Native.frameComposite(handle, index, values) == 0) return null
        val full = Bitmap.createBitmap(values, width, height, Bitmap.Config.ARGB_8888)
        return Bitmap.createScaledBitmap(full, size, size, true)
    }

    fun onionComposite(before: Int, after: Int): Bitmap? {
        val values = IntArray(width * height)
        if (Native.onionComposite(handle, before, after, values) == 0) return null
        return Bitmap.createBitmap(values, width, height, Bitmap.Config.ARGB_8888)
    }

    fun palette(max: Int = 6): List<FloatArray> {
        val values = FloatArray(max * 4)
        val written = Native.palette(handle, max, values)
        return (0 until written).map { index ->
            values.copyOfRange(index * 4, index * 4 + 4)
        }
    }

    private fun ensure(size: Int) {
        if (scratch.size < size) scratch = IntArray(size)
    }
}

data class LayerRow(
    val index: Int,
    val name: String,
    val visible: Boolean,
    val opacity: Float,
    /** 0 paint, 1 group, 2 fill. */
    val kind: Int = 0,
    val parent: Int? = null,
    val expanded: Boolean = true,
    val depth: Int = 0,
    val tag: Int = 0,
    val reference: Boolean = false,
) {
    val isGroup: Boolean get() = kind == 1
    val isFill: Boolean get() = kind == 2
}
