// The Android animation card: frame strip, playback, hold, loop and onion skin. It floats over the
// canvas, like the tool card, so the drawing stays visible while a frame is judged.

package dev.drawkit.android.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
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
import androidx.compose.runtime.getValue
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
import dev.drawkit.android.core.applyFps
import dev.drawkit.android.core.applyFrameHold
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

@Composable
internal fun AnimationPanel(state: PaintState, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(0.94f),
        shape = RoundedCornerShape(2.dp),
        color = MaterialTheme.colorScheme.surface,
        border = androidx.compose.foundation.BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
        shadowElevation = 0.dp,
    ) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Animation", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.weight(1f))
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Rounded.Close, "Close", tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            // A frame that is added, duplicated or removed slides into place, and the active frame
            // fades its tint instead of snapping it.
            LazyRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                itemsIndexed(state.frameThumbnails, key = { index, _ -> index }) { index, bitmap ->
                    val active = index == state.activeFrame
                    val tint by animateColorAsState(
                        if (active) {
                            MaterialTheme.colorScheme.primary
                        } else {
                            MaterialTheme.colorScheme.surfaceVariant
                        },
                        expressiveEffects(),
                        label = "frame",
                    )
                    val rim by animateColorAsState(
                        if (active) {
                            MaterialTheme.colorScheme.onSurface
                        } else {
                            Color.White.copy(alpha = 0.10f)
                        },
                        expressiveEffects(),
                        label = "frameRim",
                    )
                    Box(
                        modifier = Modifier
                            .size(48.dp)
                            .clip(RoundedCornerShape(7.dp))
                            .animateItem(),
                        contentAlignment = Alignment.BottomEnd,
                    ) {
                        Surface(
                            onClick = { state.selectFrame(index) },
                            shape = RoundedCornerShape(7.dp),
                            color = tint,
                            border = androidx.compose.foundation.BorderStroke(1.dp, rim),
                            modifier = Modifier
                                .size(48.dp)
                                .alpha(
                                    if (index >= state.loopStart &&
                                        index < state.loopEnd
                                    ) {
                                        1f
                                    } else {
                                        0.42f
                                    },
                                ),
                        ) {
                            if (bitmap != null) {
                                Image(bitmap.asImageBitmap(), null, Modifier.padding(2.dp))
                            }
                        }
                        Text(
                            "${index + 1}",
                            color = MaterialTheme.colorScheme.onSurface,
                            fontSize = 9.sp,
                            modifier = Modifier
                                .padding(2.dp)
                                .background(
                                    Color.Black.copy(alpha = 0.55f),
                                    RoundedCornerShape(3.dp),
                                )
                                .padding(horizontal = 3.dp, vertical = 1.dp),
                        )
                    }
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { state.stepFrame(-1) }) {
                    Icon(Icons.Rounded.SkipPrevious, "Previous frame", tint = MaterialTheme.colorScheme.onSurface)
                }
                IconButton(onClick = { state.togglePlayback() }) {
                    AnimatedContent(
                        targetState = state.isPlaying,
                        transitionSpec = { fadeIn() togetherWith fadeOut() },
                        label = "play",
                    ) { playing ->
                        Icon(
                            if (playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow,
                            if (playing) "Pause" else "Play",
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
                IconButton(onClick = { state.stepFrame(1) }) {
                    Icon(Icons.Rounded.SkipNext, "Next frame", tint = MaterialTheme.colorScheme.onSurface)
                }
                Spacer(Modifier.weight(1f))
                IconButton(onClick = { state.addFrame(false) }) {
                    Icon(Icons.Rounded.Add, "Add frame", tint = MaterialTheme.colorScheme.onSurface)
                }
                IconButton(onClick = { state.addFrame(true) }) {
                    Icon(Icons.Rounded.ContentCopy, "Duplicate frame", tint = MaterialTheme.colorScheme.onSurface)
                }
                IconButton(onClick = { state.removeFrame() }) {
                    Icon(Icons.Rounded.Delete, "Remove frame", tint = MaterialTheme.colorScheme.onSurface)
                }
            }
            MiniSlider("FPS", state.fps.toFloat(), 1f..60f, "${state.fps}") {
                state.applyFps(it.toInt())
            }
            MiniSlider("Hold", state.frameHold.toFloat(), 10f..500f, "${state.frameHold} ms") {
                state.applyFrameHold(it.toInt())
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Loop", color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f), fontSize = 11.sp)
                Switch(state.loopEnabled, { state.applyLoopEnabled(it) })
                Spacer(Modifier.weight(1f))
                Text("Onion", color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f), fontSize = 11.sp)
                Switch(state.onionEnabled, { state.applyOnionEnabled(it) })
            }
            MiniSlider(
                "Loop in",
                (state.loopStart + 1).toFloat(),
                1f..state.frameCount.toFloat(),
                "${state.loopStart + 1}",
            ) { state.applyLoopStart(it.toInt() - 1) }
            MiniSlider(
                "Loop out",
                state.loopEnd.toFloat(),
                (state.loopStart + 1).toFloat()..state.frameCount.toFloat(),
                "${state.loopEnd}",
            ) { state.applyLoopEnd(it.toInt()) }
            MiniSlider(
                "Onion before",
                state.onionBefore.toFloat(),
                0f..3f,
                "${state.onionBefore}",
            ) { state.setOnionRange(it.toInt(), state.onionAfter) }
            MiniSlider(
                "Onion after",
                state.onionAfter.toFloat(),
                0f..3f,
                "${state.onionAfter}",
            ) { state.setOnionRange(state.onionBefore, it.toInt()) }
        }
    }
}
