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
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.widget.FrameLayout
import dev.drawkit.android.core.PaintState
import dev.drawkit.android.core.segmentGpu
import dev.drawkit.android.core.attachSurface
import dev.drawkit.android.core.beginFill
import dev.drawkit.android.core.detachSurface
import dev.drawkit.android.core.present
import dev.drawkit.android.core.SelectionRect
import dev.drawkit.android.core.Tool
import kotlin.math.max
import kotlin.math.min

/**
 * The canvas. A `SurfaceView` draws the resident layers and the live stroke; an overlay View on top
 * draws the guides, the selection and the cursor and owns the touch stream.
 */
class DrawingView(context: Context) : FrameLayout(context) {
    var state: PaintState? = null
    private val surfaceView = SurfaceView(context)
    private val overlay = Overlay(context)
    private var selectionStart: PointF? = null
    private var selectionDraft: RectF? = null
    private var lastMoveTime = 0L
    /** The tool the barrel button temporarily replaced, so lifting the pen restores it. */
    private var toolBeforeBarrel: Tool? = null

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
    private val ringPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 2.5f
        color = Color.argb(220, 255, 255, 255)
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
            refresh()
            return true
        }
    })

    init {
        setBackgroundColor(Color.parseColor("#1B1917"))
        // The overlay is a document view: its guides, selection and ring are in canvas pixels and
        // must not be mirrored by an RTL chrome around it.
        layoutDirection = View.LAYOUT_DIRECTION_LTR
        addView(surfaceView, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        addView(overlay, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        surfaceView.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                state?.attachSurface(holder.surface, width.coerceAtLeast(1), height.coerceAtLeast(1))
                // Tell the compositor this surface is a drawing at the panel's fastest rate, so
                // it does not hold frames to a 60 Hz vote the way a video player would.
                if (android.os.Build.VERSION.SDK_INT >= 30) {
                    holder.surface.setFrameRate(
                        120f, android.view.Surface.FRAME_RATE_COMPATIBILITY_FIXED_SOURCE,
                    )
                }
                present()
            }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) {
                state?.gpu?.resize(w.coerceAtLeast(1), h.coerceAtLeast(1))
                present()
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                state?.detachSurface()
            }
        })
    }

    private val documentSize: Float
        get() = (state?.controller?.width ?: 1024).toFloat()

    private val fit: Float
        get() {
            val pad = 26f * resources.displayMetrics.density
            return min((width - pad * 2) / documentSize, (height - pad * 2) / documentSize)
                .coerceAtLeast(0.01f)
        }

    private fun paperRect(): FloatArray {
        val scale = fit * zoom
        val half = documentSize / 2f * scale
        return floatArrayOf(
            width / 2f + panX - half, height / 2f + panY - half, half * 2f, half * 2f,
        )
    }

    private var benchRan = false

    /**
     * `DRAWKIT_BENCH_POINTER` for this shell: `adb shell am start ... --es debug.drawkit.bench pointer`
     * paints one stroke through the same calls the touch handler makes and logs the cost of a
     * sample, then a document pixel under the stroke, which proves the paint reached the tiles.
     */
    private fun maybeRunBench() {
        if (benchRan) return
        val activity = context as? android.app.Activity ?: return
        val asked = activity.intent?.getStringExtra("debug.drawkit.bench") ?: return
        val extended = asked == "hdr"
        if (asked != "pointer" && !extended) return
        benchRan = true
        val mode = activity.intent?.getStringExtra("debug.drawkit.present")
        postDelayed({
            // The range is only known once the surface has attached; switching here rebuilds it
            // the same way the toolbar switch does.
            if (extended) state?.chooseHdr(true)
            val present = when (mode) {
                "immediate" -> 1
                "mailbox" -> 2
                else -> 0
            }
            if (mode != null) state?.gpu?.setPresentMode(present)
            postDelayed({ runPointerBench() }, 600)
        }, 900)
    }

    private fun runPointerBench() {
        val state = state ?: return
        paintPointerBench(state, documentSize, ::present)
    }

    fun present() {
        state?.present(paperRect())
    }

    /// Ask for the next frame without presenting: input marks the view dirty and the frame's draw
    /// presents once, so a pen that sends several samples between two frames does not pay for a
    /// swapchain present each time.
    fun requestFrame() {
        overlay.postInvalidateOnAnimation()
    }

    fun refresh() {
        present()
        overlay.invalidate()
    }

    fun recentre() {
        zoom = 1f
        panX = 0f
        panY = 0f
        refresh()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        present()
        maybeRunBench()
    }

    private inner class Overlay(context: Context) : View(context) {
        init {
            isClickable = true
        }

        override fun onDraw(canvas: Canvas) {
            val paintState = state ?: return
            // One present per drawn frame: the queued segments are submitted here, together. A
            // surface that never attached leaves the canvas blank; painting reports that instead
            // of drawing a CPU copy the resident stack would overwrite.
            present()
            paintState.onionBitmap?.let { drawOnion(canvas, it) }
            val scale = fit * zoom
            val half = documentSize / 2f * scale
            val cx = width / 2f + panX
            val cy = height / 2f + panY
            if (paintState.mirror != 0) {
                if (paintState.mirror and 1 != 0) {
                    canvas.drawLine(cx, cy - half, cx, cy + half, guidePaint)
                }
                if (paintState.mirror and 2 != 0) {
                    canvas.drawLine(cx - half, cy, cx + half, cy, guidePaint)
                }
            }
            paintState.selection?.let { selection ->
                val left = cx + (selection.x - documentSize / 2f) * scale
                val top = cy + (selection.y - documentSize / 2f) * scale
                canvas.drawRect(left, top, left + selection.width * scale,
                    top + selection.height * scale, selectionPaint)
            }
            selectionDraft?.let { canvas.drawRect(it, selectionPaint) }
            drawCursor(canvas, paintState)
        }

        override fun onTouchEvent(event: MotionEvent): Boolean = handleTouch(event)

        /** A hovering stylus shows the brush ring before it touches, as a drawing tablet does. */
        override fun onHoverEvent(event: MotionEvent): Boolean {
            val toolType = event.getToolType(0)
            // A tablet in "mouse mode" (most USB pens and several Bluetooth ones) reports as a
            // mouse; the ring has to follow it too, or the pen has no preview before it lands.
            val pointer = toolType == MotionEvent.TOOL_TYPE_STYLUS ||
                toolType == MotionEvent.TOOL_TYPE_ERASER ||
                toolType == MotionEvent.TOOL_TYPE_MOUSE
            if (!pointer) return super.onHoverEvent(event)
            when (event.actionMasked) {
                MotionEvent.ACTION_HOVER_ENTER, MotionEvent.ACTION_HOVER_MOVE -> {
                    lastScreenX = event.x
                    lastScreenY = event.y
                    invalidate()
                    return true
                }
                MotionEvent.ACTION_HOVER_EXIT -> {
                    lastScreenX = -1f
                    invalidate()
                    return true
                }
            }
            return super.onHoverEvent(event)
        }
    }

    /** The CPU bitmap is the whole canvas when the resident surface is unavailable or HDR. */
    /** Onion skin ghosts, drawn over the resident canvas; the canvas itself is the surface. */
    private fun drawOnion(canvas: Canvas, bitmap: android.graphics.Bitmap) {
        val scale = fit * zoom
        val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
        canvas.save()
        canvas.translate(width / 2f + panX, height / 2f + panY)
        canvas.scale(scale, scale)
        canvas.drawBitmap(bitmap, -documentSize / 2f, -documentSize / 2f, bitmapPaint)
        canvas.restore()
    }

    private fun drawCursor(canvas: Canvas, paintState: PaintState) {
        if (lastScreenX < 0 || paintState.tool == Tool.Fill ||
            paintState.tool == Tool.Eyedropper || paintState.tool == Tool.Select
        ) {
            return
        }
        val radius = paintState.activeSize / 2f * fit * zoom
        if (radius < 2f) return
        canvas.drawCircle(lastScreenX, lastScreenY, radius, ringPaint)
    }

    private fun handleTouch(event: MotionEvent): Boolean {
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

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                parent?.requestDisallowInterceptTouchEvent(true)
                if (paintState.tool == Tool.Select) {
                    selectionStart = PointF(event.x, event.y)
                    selectionDraft = RectF(event.x, event.y, event.x, event.y)
                    hasLast = false
                    overlay.invalidate()
                    return true
                }
                lastMoveTime = event.eventTime
                lastScreenX = event.x
                lastScreenY = event.y
                adoptPenTool(event, paintState)
                lastX = toCanvasX(event.x)
                lastY = toCanvasY(event.y)
                hasLast = true
                if (paintState.tool == Tool.Eyedropper) {
                    paintState.pick(lastX.toInt(), lastY.toInt())
                    hasLast = false
                } else if (paintState.tool == Tool.Fill) {
                    paintState.beginFill(lastX.toInt(), lastY.toInt())
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
                requestFrame()
            }

            MotionEvent.ACTION_MOVE -> {
                selectionStart?.let { start ->
                    selectionDraft = RectF(min(start.x, event.x), min(start.y, event.y),
                        max(start.x, event.x), max(start.y, event.y))
                    overlay.invalidate()
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
                if (x != lastX || y != lastY) lastMoveTime = event.eventTime
                val tilt = tiltOf(event, 0)
                paintState.segment(
                    lastX, lastY, x, y, pressureOf(event, 0),
                    tilt[0], tilt[1], azimuthOf(event, 0), 0f,
                    erasing(event, paintState),
                )
                lastX = x
                lastY = y
                paintState.afterSegment()
                refresh()
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
                    overlay.invalidate()
                    return true
                }
                if (hasLast) {
                    val held = event.actionMasked == MotionEvent.ACTION_UP &&
                        event.eventTime - lastMoveTime >= 350
                    paintState.finishStroke(quickShapeRequested = held)
                    hasLast = false
                    lastScreenX = event.x
                    lastScreenY = event.y
                    requestFrame()
                }
                releasePenTool(paintState)
            }
        }
        return true
    }

    private fun erasing(event: MotionEvent, paintState: PaintState): Boolean =
        paintState.tool == Tool.Eraser || event.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER

    /**
     * The pen's eraser tip is the eraser; the barrel button is a temporary eraser that gives the
     * previous tool back on lift. Both are what the stylus owner already expects, so no toolbar
     * trip is needed mid-drawing.
     */
    private fun adoptPenTool(event: MotionEvent, paintState: PaintState) {
        if (event.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER) {
            paintState.tool = Tool.Eraser
            return
        }
        // A pen tip reports the stylus button; a tablet in mouse mode maps its barrel to the
        // secondary button, so both mean "erase for as long as it is held".
        val barrel = event.buttonState and
            (MotionEvent.BUTTON_STYLUS_PRIMARY or MotionEvent.BUTTON_SECONDARY) != 0
        if (barrel && paintState.tool != Tool.Eraser) {
            toolBeforeBarrel = paintState.tool
            paintState.tool = Tool.Eraser
        }
    }

    private fun releasePenTool(paintState: PaintState) {
        toolBeforeBarrel?.let { paintState.tool = it }
        toolBeforeBarrel = null
    }

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
