package dev.drawkit.android.ui

import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import dev.drawkit.android.core.PaintState

/**
 * Mirror axes, the selection and the sheet's registration marks, in view pixels, over the
 * resident canvas.
 *
 * The painting itself is the surface underneath; the overlay view only draws these guides, so
 * they stay cheap and never become part of a frame. The corner ticks come from a printer's proof:
 * they mark the sheet's true edge without drawing a frame around it.
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
    registration: Paint,
    tick: Float,
) {
    val half = documentSize / 2f * scale
    val left = centreX - half
    val top = centreY - half
    val right = centreX + half
    val bottom = centreY + half
    val gap = tick * 0.25f
    val corners = listOf(
        floatArrayOf(left, top, -1f, -1f),
        floatArrayOf(right, top, 1f, -1f),
        floatArrayOf(left, bottom, -1f, 1f),
        floatArrayOf(right, bottom, 1f, 1f),
    )
    for (corner in corners) {
        val x = corner[0]
        val y = corner[1]
        val dx = corner[2]
        val dy = corner[3]
        canvas.drawLine(x + dx * gap, y, x + dx * (gap + tick), y, registration)
        canvas.drawLine(x, y + dy * gap, x, y + dy * (gap + tick), registration)
    }
    if (state.mirror != 0) {
        if (state.mirror and 1 != 0) {
            canvas.drawLine(centreX, top, centreX, bottom, guide)
        }
        if (state.mirror and 2 != 0) {
            canvas.drawLine(left, centreY, right, centreY, guide)
        }
    }
    state.selection?.let { selection ->
        val selectionLeft = centreX + (selection.x - documentSize / 2f) * scale
        val selectionTop = centreY + (selection.y - documentSize / 2f) * scale
        canvas.drawRect(
            selectionLeft,
            selectionTop,
            selectionLeft + selection.width * scale,
            selectionTop + selection.height * scale,
            selectionPaint,
        )
    }
    draft?.let { canvas.drawRect(it, selectionPaint) }
}
