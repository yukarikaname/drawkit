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

    external fun compositeRect(handle: Long, x: Int, y: Int, width: Int, height: Int, out: IntArray): Int
    external fun dynamicRange(handle: Long): Int
    external fun setDynamicRange(handle: Long, range: Int)

    /** Premultiplied linear extended sRGB as half floats, for `Bitmap.Config.RGBA_F16`. */
    external fun compositeLinearHalfRect(
        handle: Long,
        x: Int,
        y: Int,
        width: Int,
        height: Int,
        out: java.nio.ByteBuffer,
    ): Int
    external fun exportExr(handle: Long, path: String): Boolean
    external fun exportBrush(handle: Long, path: String): Boolean
    external fun importBrush(handle: Long, path: String): Int

    external fun frameCount(handle: Long): Int
    external fun frameActive(handle: Long): Int
    external fun frameAdd(handle: Long, duplicate: Boolean): Int
    external fun frameSelect(handle: Long, index: Int): Boolean
    external fun frameRemove(handle: Long, index: Int): Boolean
    external fun frameComposite(handle: Long, index: Int, out: IntArray): Int
    external fun onionComposite(handle: Long, before: Int, after: Int, out: IntArray): Int
    external fun fps(handle: Long): Int
    external fun setFps(handle: Long, fps: Int)
    external fun frameDuration(handle: Long): Int
    external fun setFrameDuration(handle: Long, duration: Int)
    external fun loopEnabled(handle: Long): Boolean
    external fun setLoopEnabled(handle: Long, enabled: Boolean)
    external fun loopStart(handle: Long): Int
    external fun setLoopStart(handle: Long, index: Int)
    external fun loopEnd(handle: Long): Int
    external fun setLoopEnd(handle: Long, end: Int)
    external fun takeDirty(handle: Long, out: IntArray): Int
    external fun isClear(handle: Long): Boolean

    external fun strokeBegin(handle: Long, timeMs: Long)
    external fun stroke(
        handle: Long,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        radius: Float,
        pressure: Float,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        timeMs: Long,
    )
    external fun erase(
        handle: Long,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        radius: Float,
        pressure: Float,
        timeMs: Long,
    )
    external fun strokeEx(
        handle: Long,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        radius: Float,
        pressure: Float,
        tiltX: Float,
        tiltY: Float,
        azimuth: Float,
        roll: Float,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        timeMs: Long,
    )
    external fun eraseEx(
        handle: Long,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        radius: Float,
        pressure: Float,
        tiltX: Float,
        tiltY: Float,
        azimuth: Float,
        roll: Float,
        timeMs: Long,
    )
    external fun strokeEnd(handle: Long)
    external fun fill(
        handle: Long,
        x: Int,
        y: Int,
        tolerance: Float,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        timeMs: Long,
    )
    external fun fillEx(
        handle: Long,
        x: Int,
        y: Int,
        tolerance: Float,
        close: Int,
        expand: Int,
        feather: Int,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        timeMs: Long,
    )
    external fun setBrush(handle: Long, kind: Int)
    external fun setStabilization(handle: Long, strength: Float)

    /** The eraser has its own strength; it is not the brush's flow. */
    external fun setEraserStrength(handle: Long, strength: Float)
    external fun setFillSnap(handle: Long, enabled: Boolean)

    external fun setMirror(handle: Long, mode: Int)
    external fun setAutoShape(handle: Long, enabled: Boolean)
    external fun setQuickShape(handle: Long, enabled: Boolean)
    external fun fillBegin(
        handle: Long,
        x: Int,
        y: Int,
        tolerance: Float,
        close: Int,
        expand: Int,
        feather: Int,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        timeMs: Long,
    ): Boolean
    external fun fillStep(handle: Long, progress: Float): Boolean
    external fun fillFinish(handle: Long): Boolean
    external fun fillCancel(handle: Long): Boolean
    external fun extractLineArt(handle: Long, threshold: Float, softness: Float): Boolean
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
    external fun layerAddFill(handle: Long, red: Float, green: Float, blue: Float, alpha: Float): Int
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
    external fun layerThumbnail(handle: Long, index: Int, width: Int, height: Int, out: IntArray): Int

    external fun canvasCreate(surface: android.view.Surface, width: Int, height: Int, hdr: Boolean): Long
    external fun canvasFree(handle: Long)
    external fun canvasResize(handle: Long, width: Int, height: Int)
    external fun canvasResetLayers(handle: Long, count: Int, width: Int, height: Int)
    external fun canvasSetBudget(handle: Long, bytes: Long)
    external fun canvasSetLayer(
        handle: Long,
        index: Int,
        opacity: Float,
        blend: Int,
        visible: Boolean,
        fill: Boolean,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
    )
    external fun canvasSetLayerClip(handle: Long, index: Int, clip: Int)
    external fun canvasUploadLayer(
        handle: Long,
        index: Int,
        x: Int,
        y: Int,
        width: Int,
        height: Int,
        pixels: ByteArray,
    ): Int

    /** The same rectangle as premultiplied linear Display P3 floats, for a half-float stack. */
    external fun canvasUploadLayerLinear(
        handle: Long,
        index: Int,
        x: Int,
        y: Int,
        width: Int,
        height: Int,
        pixels: FloatArray,
    ): Int

    /** Whether the swapchain took the extended-range path; false when the driver refused it. */
    external fun canvasHdr(handle: Long): Int

    /** 0 fifo, 1 immediate, 2 mailbox. False means the platform refused the mode. */
    external fun canvasSetPresentMode(handle: Long, mode: Int): Int
    external fun canvasPresentMode(handle: Long): Int
    external fun canvasPresent(handle: Long, paperX: Float, paperY: Float, paperWidth: Float, paperHeight: Float): Int
    external fun canvasStrokeBegin(
        handle: Long,
        width: Int,
        height: Int,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        flow: Float,
        pressureAlpha: Float,
        paper: Float,
        tip: Float,
        dual: Float,
        dualScale: Float,
        dualHardness: Float,
        dualRoundness: Float,
        dualAngle: Float,
        dualPaper: Float,
        dualTip: Float,
        wet: Float,
        jitter: Float,
        grain: Float,
        dualJitter: Float,
        dualGrain: Float,
        seedLow: Int,
        seedHigh: Int,
        erase: Boolean,
        previewLayer: Int,
    )
    external fun canvasStrokeSegment(
        handle: Long,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        radius: Float,
        hardness: Float,
        spacing: Float,
        roundness: Float,
        angle: Float,
        pressure: Float,
    )
    external fun canvasPresentStats(handle: Long, out: FloatArray)
    external fun canvasSetStrokeBatch(handle: Long, on: Boolean)
    external fun canvasFlushStroke(handle: Long)
    external fun canvasCommitStroke(
        editor: Long,
        canvas: Long,
        layer: Int,
        red: Float,
        green: Float,
        blue: Float,
        flow: Float,
    ): Int
    external fun canvasStrokeEnd(handle: Long)

    /** The ideal path the last stroke snaps to as `x, y` pairs; zero stays freehand. */
    external fun shapePath(handle: Long, snap: Boolean, out: FloatArray): Int

    /** One layer as premultiplied linear Display P3 floats, for a half-float upload. */
    external fun layerLinear(handle: Long, index: Int, out: FloatArray): Int
    external fun canvasMaterialize(editor: Long, canvas: Long): Int
    external fun canvasUndoTiles(handle: Long): Int
    external fun canvasRedoTiles(handle: Long): Int
    external fun canvasBrushParam(handle: Long, id: Int): Float
    external fun canvasBrushKindParam(kind: Int, id: Int): Float
    external fun canvasPick(handle: Long, x: Int, y: Int, out: FloatArray): Int
    external fun canvasStrokeSeed(handle: Long, out: IntArray): Int
    external fun canvasBrushTextureSize(handle: Long, out: IntArray): Int
    external fun canvasBrushTexture(handle: Long, out: ByteArray): Int
    external fun canvasSetBrushTexture(handle: Long, width: Int, height: Int, data: ByteArray): Int
    external fun canvasClearBrushTexture(handle: Long)
    external fun canvasMirrorSegments(
        handle: Long,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        out: FloatArray,
    ): Int
    external fun canvasSetDeferRaster(handle: Long, enabled: Boolean)
    external fun canvasFlushDeferred(handle: Long)

    /** The resident composite at canvas size, premultiplied RGBA8. */
    external fun canvasExportGpu(handle: Long, width: Int, height: Int, out: ByteArray): Int

    /** One resident layer pixel's alpha, or a negative value when the tile cannot answer. */
    external fun canvasLayerAlpha(handle: Long, index: Int, x: Int, y: Int): Float
    external fun canvasApplyFill(
        handle: Long,
        layer: Int,
        mask: FloatArray,
        width: Int,
        height: Int,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        alphaLocked: Boolean,
        under: Boolean,
        dirty: IntArray,
    ): Int
    external fun fillGpuBeginFrom(
        handle: Long,
        composite: ByteArray,
        width: Int,
        height: Int,
        x: Int,
        y: Int,
        tolerance: Float,
        close: Int,
        expand: Int,
        feather: Int,
        red: Float,
        green: Float,
        blue: Float,
        alpha: Float,
        seedRed: Float,
        seedGreen: Float,
        seedBlue: Float,
        seedAlpha: Float,
        under: Boolean,
        timeMs: Long,
        mask: FloatArray,
        rect: IntArray,
        colour: FloatArray,
    ): Boolean
    external fun fillGpuFinish(handle: Long, rects: IntArray, count: Int): Boolean
    external fun fillGpuCancel(handle: Long): Boolean
    external fun undoIsGpu(handle: Long): Int
    external fun redoIsGpu(handle: Long): Int
    external fun layerRect(handle: Long, index: Int, x: Int, y: Int, width: Int, height: Int, out: ByteArray): Int
    external fun layerFill(handle: Long, index: Int, out: FloatArray): Int
    external fun pick(handle: Long, x: Int, y: Int, out: FloatArray): Int
    external fun palette(handle: Long, max: Int, out: FloatArray): Int
}
