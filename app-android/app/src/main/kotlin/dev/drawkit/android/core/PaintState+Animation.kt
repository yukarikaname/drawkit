// Frame selection, playback and onion skin. The state fields live in PaintState; the behaviour
// sits here so the main window class stays readable.

package dev.drawkit.android.core

import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

    // MARK: Animation

    fun PaintState.refreshFrames() {
        frameCount = maxOf(controller.frameCount(), 1)
        activeFrame = controller.frameActive()
        fps = controller.fps()
        frameHold = controller.frameDuration()
        loopEnabled = controller.loopEnabled()
        loopStart = controller.loopStart()
        loopEnd = controller.loopEnd()
        frameThumbnails = (0 until frameCount).map { controller.frameThumbnail(it) }
        refreshOnion()
        revision++
        onInvalidate?.invoke()
    }

    fun PaintState.refreshOnion() {
        onionBitmap = if (onionEnabled && frameCount > 1) {
            controller.onionComposite(onionBefore, onionAfter)
        } else {
            null
        }
        revision++
        onInvalidate?.invoke()
    }

    fun PaintState.selectFrame(index: Int, refresh: Boolean = true) {
        if (refresh) stopPlayback()
        if (!controller.frameSelect(index)) return
        controller.full()
        refreshLayers()
        if (refresh) {
            refreshFrames()
        } else {
            activeFrame = index
            revision++
            onInvalidate?.invoke()
        }
    }

    fun PaintState.stepFrame(delta: Int) {
        stopPlayback()
        val start = loopStart.coerceIn(0, frameCount - 1)
        val end = if (loopEnd <= 0 || loopEnd > frameCount) frameCount else maxOf(loopEnd, start + 1)
        var next = if (activeFrame < start || activeFrame >= end) {
            if (delta >= 0) start else end - 1
        } else {
            activeFrame + delta
        }
        if (next >= end) next = start
        if (next < start) next = end - 1
        selectFrame(next)
    }

    fun PaintState.addFrame(duplicate: Boolean) {
        stopPlayback()
        if (controller.frameAdd(duplicate) < 0) return
        controller.full()
        refreshLayers()
        refreshFrames()
    }

    fun PaintState.removeFrame() {
        stopPlayback()
        if (!controller.frameRemove(activeFrame)) return
        controller.full()
        refreshLayers()
        refreshFrames()
    }

    fun PaintState.applyFps(value: Int) {
        fps = value.coerceIn(1, 60)
        controller.setFps(fps)
    }

    fun PaintState.applyFrameHold(value: Int) {
        frameHold = value.coerceIn(10, 2000)
        controller.setFrameDuration(frameHold)
    }

    fun PaintState.applyLoopEnabled(enabled: Boolean) {
        loopEnabled = enabled
        controller.setLoopEnabled(enabled)
    }

    fun PaintState.applyLoopStart(value: Int) {
        controller.setLoopStart(value)
        loopStart = controller.loopStart()
    }

    fun PaintState.applyLoopEnd(value: Int) {
        controller.setLoopEnd(value)
        loopEnd = controller.loopEnd()
    }

    fun PaintState.applyOnionEnabled(enabled: Boolean) {
        onionEnabled = enabled
        refreshOnion()
    }

    fun PaintState.setOnionRange(before: Int, after: Int) {
        onionBefore = before.coerceIn(0, 3)
        onionAfter = after.coerceIn(0, 3)
        refreshOnion()
    }

    fun PaintState.togglePlayback() {
        if (isPlaying) stopPlayback() else startPlayback()
    }

    private fun PaintState.startPlayback() {
        if (frameCount <= 1 || playbackJob != null) return
        isPlaying = true
        onionBitmap = null
        playbackJob = scope.launch {
            while (isActive) {
                delay(controller.frameDuration().toLong().coerceAtLeast(10))
                val start = loopStart.coerceIn(0, frameCount - 1)
                val end = if (loopEnd <= 0 || loopEnd > frameCount) frameCount else maxOf(loopEnd, start + 1)
                var next = activeFrame + 1
                if (next >= end) {
                    if (!loopEnabled) break
                    next = start
                }
                selectFrame(next, refresh = false)
            }
            isPlaying = false
            playbackJob = null
            refreshFrames()
        }
    }

    fun PaintState.stopPlayback() {
        playbackJob?.cancel()
        playbackJob = null
        if (isPlaying) {
            isPlaying = false
            refreshFrames()
        }
    }
