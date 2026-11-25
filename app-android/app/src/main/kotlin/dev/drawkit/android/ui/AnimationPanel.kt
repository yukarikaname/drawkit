// The Android animation card: frame strip, playback, hold, loop and onion skin. It floats over the
// canvas, like the tool card, so the drawing stays visible while a frame is judged.

package dev.drawkit.android.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.SkipPrevious
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.drawkit.android.core.PaintState
import dev.drawkit.android.core.addFrame
import dev.drawkit.android.core.applyFrameHold
import dev.drawkit.android.core.applyFps
import dev.drawkit.android.core.applyLoopEnabled
import dev.drawkit.android.core.applyLoopEnd
import dev.drawkit.android.core.applyLoopStart
import dev.drawkit.android.core.applyOnionEnabled
import dev.drawkit.android.core.refreshFrames
import dev.drawkit.android.core.removeFrame
import dev.drawkit.android.core.selectFrame
import dev.drawkit.android.core.setOnionRange
import dev.drawkit.android.core.stepFrame
import dev.drawkit.android.core.togglePlayback
import dev.drawkit.android.ui.theme.Brand

@Composable
internal fun AnimationPanel(state: PaintState, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(0.94f),
        shape = RoundedCornerShape(20.dp),
        color = Brand.Carbon.copy(alpha = 0.96f),
        border = androidx.compose.foundation.BorderStroke(1.dp, Color.White.copy(alpha = 0.07f)),
        shadowElevation = 14.dp,
    ) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Animation", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.weight(1f))
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Rounded.Close, "Close", tint = Brand.Muted)
                }
            }
            Row(
                modifier = Modifier.horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                state.frameThumbnails.forEachIndexed { index, bitmap ->
                    Box(
                        modifier = Modifier
                            .size(48.dp)
                            .clip(RoundedCornerShape(7.dp))
                            .then(
                                if (index == state.activeFrame) {
                                    Modifier.padding(0.dp)
                                } else {
                                    Modifier
                                }
                            ),
                        contentAlignment = Alignment.BottomEnd,
                    ) {
                        Surface(
                            onClick = { state.selectFrame(index) },
                            shape = RoundedCornerShape(7.dp),
                            color = if (index == state.activeFrame) Brand.Vermilion else Brand.Ink,
                            border = androidx.compose.foundation.BorderStroke(
                                1.dp,
                                if (index == state.activeFrame) Brand.Paper
                                else Color.White.copy(alpha = 0.10f),
                            ),
                            modifier = Modifier
                                .size(48.dp)
                                .alpha(if (index >= state.loopStart &&
                                           index < state.loopEnd) 1f else 0.42f),
                        ) {
                            if (bitmap != null) {
                                Image(bitmap.asImageBitmap(), null, Modifier.padding(2.dp))
                            }
                        }
                        Text(
                            "${index + 1}",
                            color = Brand.Paper,
                            fontSize = 9.sp,
                            modifier = Modifier
                                .padding(2.dp)
                                .background(Color.Black.copy(alpha = 0.55f),
                                            RoundedCornerShape(3.dp))
                                .padding(horizontal = 3.dp, vertical = 1.dp),
                        )
                    }
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { state.stepFrame(-1) }) {
                    Icon(Icons.Rounded.SkipPrevious, "Previous frame", tint = Brand.Paper)
                }
                IconButton(onClick = { state.togglePlayback() }) {
                    Icon(if (state.isPlaying) Icons.Rounded.Pause else Icons.Rounded.PlayArrow,
                        "Play", tint = Brand.Vermilion)
                }
                IconButton(onClick = { state.stepFrame(1) }) {
                    Icon(Icons.Rounded.SkipNext, "Next frame", tint = Brand.Paper)
                }
                Spacer(Modifier.weight(1f))
                IconButton(onClick = { state.addFrame(false) }) {
                    Icon(Icons.Rounded.Add, "Add frame", tint = Brand.Paper)
                }
                IconButton(onClick = { state.addFrame(true) }) {
                    Icon(Icons.Rounded.ContentCopy, "Duplicate frame", tint = Brand.Paper)
                }
                IconButton(onClick = { state.removeFrame() }) {
                    Icon(Icons.Rounded.Delete, "Remove frame", tint = Brand.Paper)
                }
            }
            MiniSlider("FPS", state.fps.toFloat(), 1f..60f, "${state.fps}") {
                state.applyFps(it.toInt())
            }
            MiniSlider("Hold", state.frameHold.toFloat(), 10f..500f, "${state.frameHold} ms") {
                state.applyFrameHold(it.toInt())
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Loop", color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
                Switch(state.loopEnabled, { state.applyLoopEnabled(it) })
                Spacer(Modifier.weight(1f))
                Text("Onion", color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
                Switch(state.onionEnabled, { state.applyOnionEnabled(it) })
            }
            MiniSlider("Loop in", (state.loopStart + 1).toFloat(), 1f..state.frameCount.toFloat(),
                "${state.loopStart + 1}") { state.applyLoopStart(it.toInt() - 1) }
            MiniSlider("Loop out", state.loopEnd.toFloat(),
                (state.loopStart + 1).toFloat()..state.frameCount.toFloat(),
                "${state.loopEnd}") { state.applyLoopEnd(it.toInt()) }
            MiniSlider("Onion before", state.onionBefore.toFloat(), 0f..3f,
                "${state.onionBefore}") { state.setOnionRange(it.toInt(), state.onionAfter) }
            MiniSlider("Onion after", state.onionAfter.toFloat(), 0f..3f,
                "${state.onionAfter}") { state.setOnionRange(state.onionBefore, it.toInt()) }
        }
    }
}
