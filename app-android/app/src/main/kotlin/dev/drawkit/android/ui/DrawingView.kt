package dev.drawkit.android.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.DashPathEffect
import android.graphics.Paint
import android.graphics.PointF
import android.graphics.RectF
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import dev.drawkit.android.core.PaintState
import dev.drawkit.android.core.SelectionRect
import dev.drawkit.android.core.Tool
import kotlin.math.max
import kotlin.math.min

/**
 * The canvas. A View rather than a Compose Canvas because touch samples, historical points and
 * stylus pressure all arrive here first; the bitmap it draws is the core's composite.
 */
class DrawingView(context: Context) : View(context) {
    var state: PaintState? = null
    private var selectionStart: PointF? = null
    private var selectionDraft: RectF? = null

    private val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
    private val paperPaint = Paint().apply { color = Color.WHITE }
    private val shadowPaint = Paint().apply {
        color = Color.argb(90, 0, 0, 0)
        maskFilter = android.graphics.BlurMaskFilter(18f, android.graphics.BlurMaskFilter.Blur.NORMAL)
    }
    private val ringPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 2.5f
        color = Color.argb(220, 255, 255, 255)
    }
    private val selectionPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 2f
        color = Color.WHITE
        pathEffect = DashPathEffect(floatArrayOf(6f, 5f), 0f)
    }
    private val guidePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 1.5f
        color = Color.argb(190, 255, 140, 0)
        pathEffect = DashPathEffect(floatArrayOf(6f, 6f), 0f)
    }

    private var zoom = 1f
    private var panX = 0f
    private var panY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var hasLast = false
    private var lastScreenX = -1f
    private var lastScreenY = -1f

    private val scaler = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(detector: ScaleGestureDetector): Boolean {
            zoom = (zoom * detector.scaleFactor).coerceIn(0.35f, 10f)
            invalidate()
            return true
        }
    })

    private val documentSize: Float
        get() = (state?.controller?.width ?: 1024).toFloat()

    private val fit: Float
        get() {
            val pad = 26f * resources.displayMetrics.density
            return min((width - pad * 2) / documentSize, (height - pad * 2) / documentSize)
                .coerceAtLeast(0.01f)
        }

    fun recentre() {
        zoom = 1f
        panX = 0f
        panY = 0f
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        canvas.drawColor(Color.parseColor("#1B1917"))
        val paintState = state ?: return
        val scale = fit * zoom
        val half = documentSize / 2f * scale
        val cx = width / 2f + panX
        val cy = height / 2f + panY

        canvas.drawRoundRect(
            RectF(cx - half - 7f, cy - half - 5f, cx + half + 7f, cy + half + 10f),
            9f, 9f, shadowPaint,
        )
        canvas.save()
        canvas.translate(cx, cy)
        canvas.scale(scale, scale)
        canvas.drawRect(-documentSize / 2f, -documentSize / 2f,
                        documentSize / 2f, documentSize / 2f, paperPaint)
        canvas.drawBitmap(paintState.controller.bitmap, -documentSize / 2f, -documentSize / 2f, bitmapPaint)
        canvas.restore()

        if (paintState.mirror != 0) {
            if (paintState.mirror and 1 != 0) {
                canvas.drawLine(cx, cy - half, cx, cy + half, guidePaint)
            }
            if (paintState.mirror and 2 != 0) {
                canvas.drawLine(cx - half, cy, cx + half, cy, guidePaint)
            }
        }

        paintState.selection?.let { selection ->
            val scale = fit * zoom
            val left = width / 2f + panX + (selection.x - documentSize / 2f) * scale
            val top = height / 2f + panY + (selection.y - documentSize / 2f) * scale
            canvas.drawRect(left, top, left + selection.width * scale,
                            top + selection.height * scale, selectionPaint)
        }
        selectionDraft?.let { canvas.drawRect(it, selectionPaint) }
        drawCursor(canvas, paintState)
    }

    private fun drawCursor(canvas: Canvas, paintState: PaintState) {
        if (lastScreenX < 0 || paintState.tool == Tool.Fill || paintState.tool == Tool.Eyedropper || paintState.tool == Tool.Select) return
        val radius = paintState.brushSize / 2f * fit * zoom
        if (radius < 2f) return
        canvas.drawCircle(lastScreenX, lastScreenY, radius, ringPaint)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        val paintState = state ?: return false
        scaler.onTouchEvent(event)

        if (event.pointerCount >= 2) {
            if (hasLast) {
                paintState.controller.end()
                hasLast = false
                paintState.finishStroke()
            }
            lastScreenX = -1f
            return true
        }

        val density = resources.displayMetrics.density
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                parent?.requestDisallowInterceptTouchEvent(true)
                if (paintState.tool == Tool.Select) {
                    selectionStart = PointF(event.x, event.y)
                    selectionDraft = RectF(event.x, event.y, event.x, event.y)
                    hasLast = false
                    invalidate()
                    return true
                }
                lastScreenX = event.x
                lastScreenY = event.y
                lastX = toCanvasX(event.x)
                lastY = toCanvasY(event.y)
                hasLast = true
                if (paintState.tool == Tool.Eyedropper) {
                    paintState.pick(lastX.toInt(), lastY.toInt())
                    hasLast = false
                } else if (paintState.tool == Tool.Fill) {
                    paintState.fill(lastX.toInt(), lastY.toInt())
                    hasLast = false
                } else {
                    paintState.beginStroke()
                    val tilt = tiltOf(event, 0)
                    paintState.segment(
                        lastX, lastY, lastX, lastY, pressureOf(event, 0),
                        tilt[0], tilt[1], azimuthOf(event, 0), 0f,
                        erasing(event, paintState),
                    )
                    paintState.afterSegment()
                }
                invalidate()
            }

            MotionEvent.ACTION_MOVE -> {
                selectionStart?.let { start ->
                    selectionDraft = RectF(min(start.x, event.x), min(start.y, event.y),
                                           max(start.x, event.x), max(start.y, event.y))
                    invalidate()
                    return true
                }
                if (!hasLast) return true
                lastScreenX = event.x
                lastScreenY = event.y
                for (index in 0 until event.historySize) {
                    val x = toCanvasX(event.getHistoricalX(0, index))
                    val y = toCanvasY(event.getHistoricalY(0, index))
                    val tilt = tiltOf(event, index)
                    paintState.segment(
                        lastX, lastY, x, y, pressureOf(event, index),
                        tilt[0], tilt[1], azimuthOf(event, index), 0f,
                        erasing(event, paintState),
                    )
                    lastX = x
                    lastY = y
                }
                val x = toCanvasX(event.x)
                val y = toCanvasY(event.y)
                val tilt = tiltOf(event, 0)
                paintState.segment(
                    lastX, lastY, x, y, pressureOf(event, 0),
                    tilt[0], tilt[1], azimuthOf(event, 0), 0f,
                    erasing(event, paintState),
                )
                lastX = x
                lastY = y
                paintState.afterSegment()
                invalidate()
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                selectionStart?.let {
                    val left = toCanvasX(it.x).toInt().coerceIn(0, paintState.controller.width)
                    val top = toCanvasY(it.y).toInt().coerceIn(0, paintState.controller.height)
                    val right = toCanvasX(event.x).toInt().coerceIn(0, paintState.controller.width)
                    val bottom = toCanvasY(event.y).toInt().coerceIn(0, paintState.controller.height)
                    paintState.chooseSelection(
                        SelectionRect(min(left, right), min(top, bottom),
                                      kotlin.math.abs(right - left), kotlin.math.abs(bottom - top))
                    )
                    selectionStart = null
                    selectionDraft = null
                    invalidate()
                    return true
                }
                if (hasLast) {
                    paintState.finishStroke()
                    hasLast = false
                    lastScreenX = event.x
                    lastScreenY = event.y
                    invalidate()
                }
            }
        }
        return true
    }

    private fun erasing(event: MotionEvent, paintState: PaintState): Boolean =
        paintState.tool == Tool.Eraser || event.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER

    /// `AXIS_TILT` is the angle from the screen normal; convert it and the orientation into the
    /// same normalized tilt vector the Apple Pencil sends.
    private fun tiltOf(event: MotionEvent, historyIndex: Int): FloatArray {
        val stylus = event.getToolType(0) == MotionEvent.TOOL_TYPE_STYLUS ||
            event.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER
        if (!stylus) return floatArrayOf(0f, 0f)
        val tilt = if (historyIndex < event.historySize) {
            event.getHistoricalAxisValue(MotionEvent.AXIS_TILT, 0, historyIndex)
        } else {
            event.getAxisValue(MotionEvent.AXIS_TILT)
        }
        val lean = (1f - tilt / (Math.PI.toFloat() / 2f)).coerceIn(0f, 1f)
        val azimuth = azimuthOf(event, historyIndex)
        return floatArrayOf(
            (kotlin.math.cos(azimuth.toDouble()) * lean).toFloat(),
            (kotlin.math.sin(azimuth.toDouble()) * lean).toFloat(),
        )
    }

    private fun azimuthOf(event: MotionEvent, historyIndex: Int): Float =
        if (historyIndex < event.historySize) {
            event.getHistoricalAxisValue(MotionEvent.AXIS_ORIENTATION, 0, historyIndex)
        } else {
            event.getAxisValue(MotionEvent.AXIS_ORIENTATION)
        }

    private fun pressureOf(event: MotionEvent, historyIndex: Int): Float {
        val raw = if (historyIndex < event.historySize) {
            event.getHistoricalPressure(0, historyIndex)
        } else {
            event.pressure
        }
        return raw.coerceIn(0.05f, 1f)
    }

    private fun toCanvasX(viewX: Float): Float {
        val scale = fit * zoom
        return (viewX - width / 2f - panX) / scale + documentSize / 2f
    }

    private fun toCanvasY(viewY: Float): Float {
        val scale = fit * zoom
        return (viewY - height / 2f - panY) / scale + documentSize / 2f
    }
}
