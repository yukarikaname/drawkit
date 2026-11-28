package dev.drawkit.android.ui

import android.graphics.Color
import android.graphics.DashPathEffect
import android.graphics.Paint

/**
 * The four marks the overlay draws over the resident canvas: the selection box, the mirror axes,
 * the sheet's registration ticks and the brush ring under the pen.
 *
 * They are achromatic like the rest of the chrome; only the paint is allowed colour.
 */
internal class CanvasPencils(density: Float) {
    val selection = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 1.5f
        color = 0xFFF4F1EA.toInt()
        pathEffect = DashPathEffect(floatArrayOf(6f, 5f), 0f)
    }
    val guide = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 1f
        color = Color.argb(150, 236, 234, 228)
        pathEffect = DashPathEffect(floatArrayOf(6f, 6f), 0f)
    }
    val registration = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = density
        color = Color.argb(90, 236, 234, 228)
    }
    val ring = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 2f
        color = Color.argb(210, 236, 234, 228)
    }

    /** A registration tick's length: 12dp, so it holds its weight on any density. */
    val tick = density * 12f
}
