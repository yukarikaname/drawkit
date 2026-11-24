package dev.drawkit.android.core

import android.graphics.Bitmap
import android.os.SystemClock

/** One document, one handle and one bitmap. The bitmap is the only copy the UI draws. */
class PaintController(val width: Int = 1024, val height: Int = 1024) {
    private val handle: Long = Native.newEditor(width, height)
    val bitmap: Bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)

    private var scratch = IntArray(0)
    private val dirty = IntArray(4)
    private var time = SystemClock.uptimeMillis()

    init {
        full()
    }

    /** Release the native document; the bitmap stays valid but stops changing. */
    fun release() {
        Native.free(handle)
    }

    /** Recomposite the whole document. Used on first paint and after history or layer changes. */
    fun full() {
        ensure(width * height)
        if (Native.compositeRect(handle, 0, 0, width, height, scratch) != 0) {
            bitmap.setPixels(scratch, 0, width, 0, 0, width, height)
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

    fun fill(x: Int, y: Int, tolerance: Float, close: Int, expand: Int, feather: Int,
             color: androidx.compose.ui.graphics.Color) {
        time = maxOf(time + 1, SystemClock.uptimeMillis())
        Native.fillEx(
            handle, x, y, tolerance, close, expand, feather,
            color.red, color.green, color.blue, color.alpha, time,
        )
    }

    fun setStabilization(strength: Float) {
        Native.setStabilization(handle, strength)
    }

    fun undo(): Boolean = Native.undo(handle) != 0
    fun redo(): Boolean = Native.redo(handle) != 0
    fun canUndo(): Boolean = Native.canUndo(handle)
    fun canRedo(): Boolean = Native.canRedo(handle)
    fun isClear(): Boolean = Native.isClear(handle)

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
            )
        }
    }

    fun activeLayer(): Int = Native.layerActive(handle)
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
    fun removeLayer(): Int = Native.layerRemove(handle)
    fun setVisible(index: Int, visible: Boolean) = Native.layerSetVisible(handle, index, visible)
    fun setOpacity(index: Int, opacity: Float) = Native.layerSetOpacity(handle, index, opacity)

    fun thumbnail(index: Int, size: Int = 52): Bitmap? {
        val values = IntArray(size * size)
        if (Native.layerThumbnail(handle, index, size, size, values) == 0) return null
        return Bitmap.createBitmap(values, size, size, Bitmap.Config.ARGB_8888)
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
) {
    val isGroup: Boolean get() = kind == 1
    val isFill: Boolean get() = kind == 2
}
