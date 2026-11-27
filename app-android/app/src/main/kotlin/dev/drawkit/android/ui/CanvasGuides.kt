package dev.drawkit.android.ui

import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import dev.drawkit.android.core.PaintState

/**
 * Mirror axes and the selection, in view pixels, over the resident canvas.
 *
 * The painting itself is the surface underneath; the overlay view only draws these guides, so
 * they stay cheap and never become part of a frame.
 */
internal fun drawGuides(
    canvas: Canvas,
    state: PaintState,
    draft: RectF?,
    centreX: Float,
    centreY: Float,
    scale: Float,
    documentSize: Float,
    guide: Paint,
    selectionPaint: Paint,
) {
    val half = documentSize / 2f * scale
    if (state.mirror != 0) {
        if (state.mirror and 1 != 0) {
            canvas.drawLine(centreX, centreY - half, centreX, centreY + half, guide)
        }
        if (state.mirror and 2 != 0) {
            canvas.drawLine(centreX - half, centreY, centreX + half, centreY, guide)
        }
    }
    state.selection?.let { selection ->
        val left = centreX + (selection.x - documentSize / 2f) * scale
        val top = centreY + (selection.y - documentSize / 2f) * scale
        canvas.drawRect(
            left,
            top,
            left + selection.width * scale,
            top + selection.height * scale,
            selectionPaint,
        )
    }
    draft?.let { canvas.drawRect(it, selectionPaint) }
}
