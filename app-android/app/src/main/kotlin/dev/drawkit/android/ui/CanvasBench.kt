package dev.drawkit.android.ui

import dev.drawkit.android.core.PaintState

/** The per-brush pointer bench: samples and frames through the same calls the touch handler makes. */
internal fun paintPointerBench(state: PaintState, documentSize: Float, present: () -> Unit) {
    // One line per brush, the same conditions for each: 1024² canvas, 48 samples across a
    // stroke, then a frame. `updates/s` is what a sustained stroke can do when every frame
    // carries the samples since the last one.
    val names = arrayOf(
        "SoftRound", "Pencil", "Ink", "Marker", "Airbrush",
        "Bristle", "Spray", "Chalk", "Watercolor",
    )
    // Quick Shape is on by default and keeps the stroke on the CPU path, so the numbers below
    // would describe that path. Turn it off for the GPU measurement.
    state.quickShape = false
    state.autoShape = false
    for (kind in names.indices) {
        state.chooseBrush(kind)
        state.beginStroke()
        val onGpu = state.gpuStroke
        state.finishStroke()
        val side = documentSize
        val y = side * 0.4f
        val x0 = side * 0.35f
        val x1 = side * 0.65f
        val samples = ArrayList<Double>(48)
        var last = x0
        state.beginStroke()
        for (step in 1..48) {
            val x = x0 + (x1 - x0) * step / 48f
            val began = System.nanoTime()
            state.segment(last, y, x, y, 1f, 0f, 0f, 0f, 0f, false)
            state.afterSegment()
            samples.add((System.nanoTime() - began) / 1e6)
            last = x
        }
        state.finishStroke()
        val frames = ArrayList<Double>(12)
        for (frame in 0 until 12) {
            val began = System.nanoTime()
            present()
            frames.add((System.nanoTime() - began) / 1e6)
        }
        // Back-to-back presents block on the swapchain's image count, which a stroke never
        // does: a stroke presents once per display frame. Sleep a frame between these so the
        // number describes a pen, not a queue that is already full.
        val paced = ArrayList<Double>(12)
        for (frame in 0 until 12) {
            Thread.sleep(6)
            val began = System.nanoTime()
            present()
            paced.add((System.nanoTime() - began) / 1e6)
        }
        samples.sort()
        frames.sort()
        paced.sort()
        val sample = samples[samples.size / 2]
        val frame = frames[frames.size / 2]
        val pacedFrame = paced[paced.size / 2]
        val rate = 1000.0 / maxOf(sample, pacedFrame)
        // Where the frame went: composite, copy, draw, drawable acquire, draw encode.
        val parts = state.gpu?.presentStats() ?: FloatArray(8)
        android.util.Log.i(
            "DRAWKIT_ANDROID_BRUSH",
            ("%s canvas %d gpu=%b hdr=%b swapHdr=%b present=%d" +
                " sample p50 %.2f ms present p50 %.2f ms paced p50 %.2f ms" +
                " updates/s %.0f" +
                " | composite %.2f copy %.2f draw %.2f" +
                " (acquire %.2f views %.2f uniform %.2f frame %.2f)")
                .format(names[kind], side.toInt(), onGpu, state.hdr,
                    state.gpu?.hdr == true, state.gpu?.presentMode ?: 0,
                    sample, frame, pacedFrame, rate,
                    parts[0], parts[1], parts[2], parts[3], parts[5], parts[6], parts[4]),
        )
    }
    android.util.Log.i("DRAWKIT_ANDROID_BRUSH", "done")
}

    /**
     * `DRAWKIT_BENCH_POINTER` for this shell: `adb shell am start ... --es debug.drawkit.bench pointer`
     * paints one stroke through the same calls the touch handler makes and logs the cost of a
     * sample, then a document pixel under the stroke, which proves the paint reached the tiles.
     */
internal fun DrawingView.maybeRunBench() {
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

private fun DrawingView.runPointerBench() {
    val state = state ?: return
    paintPointerBench(state, documentSize) { present() }
}
