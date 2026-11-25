// MyPaint brush files. Import writes the params into the core; the shell only adopts the kind so
// the picker label follows without calling chooseBrush, which would reset them.

package dev.drawkit.android.core

fun PaintState.importBrush(path: String): Boolean {
    val kind = controller.importBrush(path)
    if (kind < 0) return false
    brushKind = kind
    revision++
    return true
}

fun PaintState.exportBrush(path: String): Boolean = controller.exportBrush(path)
