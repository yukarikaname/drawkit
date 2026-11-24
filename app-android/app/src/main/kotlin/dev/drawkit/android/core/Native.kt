package dev.drawkit.android.core

/**
 * The Rust painting core behind JNI. The shell keeps one bitmap; every other call asks the core
 * for pixels or changes them. A negative or missing handle is a no-op in Rust, so the declarations
 * stay simple.
 */
object Native {
    init {
        System.loadLibrary("drawkit_jni")
    }

    external fun newEditor(width: Int, height: Int): Long
    /** One label from the shared table; the fallback is the key itself. */
    external fun text(key: String, language: String): String
    external fun free(handle: Long)

    external fun compositeRect(
        handle: Long, x: Int, y: Int, width: Int, height: Int, out: IntArray,
    ): Int
    external fun takeDirty(handle: Long, out: IntArray): Int
    external fun isClear(handle: Long): Boolean

    external fun strokeBegin(handle: Long, timeMs: Long)
    external fun stroke(
        handle: Long,
        fromX: Float, fromY: Float, toX: Float, toY: Float,
        radius: Float, pressure: Float,
        red: Float, green: Float, blue: Float, alpha: Float,
        timeMs: Long,
    )
    external fun erase(
        handle: Long,
        fromX: Float, fromY: Float, toX: Float, toY: Float,
        radius: Float, pressure: Float, timeMs: Long,
    )
    external fun strokeEx(
        handle: Long,
        fromX: Float, fromY: Float, toX: Float, toY: Float,
        radius: Float, pressure: Float,
        tiltX: Float, tiltY: Float, azimuth: Float, roll: Float,
        red: Float, green: Float, blue: Float, alpha: Float,
        timeMs: Long,
    )
    external fun eraseEx(
        handle: Long,
        fromX: Float, fromY: Float, toX: Float, toY: Float,
        radius: Float, pressure: Float,
        tiltX: Float, tiltY: Float, azimuth: Float, roll: Float,
        timeMs: Long,
    )
    external fun strokeEnd(handle: Long)
    external fun fill(
        handle: Long, x: Int, y: Int, tolerance: Float,
        red: Float, green: Float, blue: Float, alpha: Float, timeMs: Long,
    )
    external fun fillEx(
        handle: Long, x: Int, y: Int, tolerance: Float,
        close: Int, expand: Int, feather: Int,
        red: Float, green: Float, blue: Float, alpha: Float, timeMs: Long,
    )
    external fun setStabilization(handle: Long, strength: Float)

    external fun setMirror(handle: Long, mode: Int)
    external fun setAutoShape(handle: Long, enabled: Boolean)
    external fun autoShape(handle: Long): Boolean
    external fun setSelection(handle: Long, x: Int, y: Int, width: Int, height: Int)
    external fun clearSelection(handle: Long)
    external fun extractSelection(handle: Long): Boolean
    external fun undo(handle: Long): Int
    external fun redo(handle: Long): Int
    external fun canUndo(handle: Long): Boolean
    external fun canRedo(handle: Long): Boolean
    external fun clear(handle: Long, red: Float, green: Float, blue: Float, alpha: Float)

    external fun layerCount(handle: Long): Int
    external fun layerActive(handle: Long): Int
    external fun layerSelect(handle: Long, index: Int)
    external fun layerAdd(handle: Long): Int
    external fun layerAddGroup(handle: Long): Int
    external fun layerAddFill(
        handle: Long, red: Float, green: Float, blue: Float, alpha: Float,
    ): Int
    external fun layerKind(handle: Long, index: Int): Int
    external fun layerParent(handle: Long, index: Int): Int
    external fun layerExpanded(handle: Long, index: Int): Boolean
    external fun layerSetExpanded(handle: Long, index: Int, expanded: Boolean)
    external fun layerMove(handle: Long, index: Int, steps: Int): Int
    external fun layerRemove(handle: Long): Int
    external fun layerVisible(handle: Long, index: Int): Boolean
    external fun layerSetVisible(handle: Long, index: Int, visible: Boolean)
    external fun layerOpacity(handle: Long, index: Int): Float
    external fun layerSetOpacity(handle: Long, index: Int, opacity: Float)
    external fun layerName(handle: Long, index: Int): String?
    external fun layerReference(handle: Long, index: Int): Boolean
    external fun layerSetReference(handle: Long, index: Int, reference: Boolean)
    external fun layerTag(handle: Long, index: Int): Int
    external fun layerSetTag(handle: Long, index: Int, tag: Int)
    external fun layerSetName(handle: Long, index: Int, name: String)
    external fun layerThumbnail(
        handle: Long, index: Int, width: Int, height: Int, out: IntArray,
    ): Int

    external fun pick(handle: Long, x: Int, y: Int, out: FloatArray): Int
    external fun palette(handle: Long, max: Int, out: FloatArray): Int
}
