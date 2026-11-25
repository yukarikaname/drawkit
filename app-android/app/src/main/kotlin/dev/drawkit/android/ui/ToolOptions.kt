package dev.drawkit.android.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.drawkit.android.core.BrushPreset
import dev.drawkit.android.core.PaintState
import dev.drawkit.android.core.Tool
import dev.drawkit.android.ui.theme.Brand

@Composable
internal fun ToolOptions(state: PaintState, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.width(176.dp),
        shape = RoundedCornerShape(18.dp),
        color = Brand.Carbon.copy(alpha = 0.9f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.06f)),
        shadowElevation = 8.dp,
    ) {
        Column(
            modifier = Modifier
                .padding(horizontal = 14.dp, vertical = 12.dp)
                .animateContentSize(animationSpec = expressiveSpatial()),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(state.tool.label, style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.weight(1f))
                Text(
                    if (state.tool == Tool.Fill || state.tool == Tool.Eyedropper ||
                        state.tool == Tool.Select) "tap"
                    else "${state.brushSize.toInt()} px",
                    color = Brand.Muted,
                    fontSize = 11.sp,
                )
            }
            if (state.tool == Tool.Select) {
                Text("Drag a rectangle on the canvas", color = Brand.Muted, fontSize = 11.sp,
                     modifier = Modifier.padding(top = 8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.padding(top = 6.dp)) {
                    Button(onClick = { state.extractSelection() }) { Text("Extract") }
                    Button(onClick = { state.chooseSelection(null) }) { Text("Deselect") }
                    Button(onClick = { state.clearLayer() }) { Text("Clear") }
                }
            } else if (state.tool == Tool.Eyedropper) {
                Text(
                    "Tap the canvas to sample a colour",
                    color = Brand.Muted,
                    fontSize = 11.sp,
                    modifier = Modifier.padding(top = 8.dp, bottom = 4.dp),
                )
            } else if (state.tool == Tool.Fill) {
                MiniSlider(
                    label = "Tolerance",
                    value = state.tolerance,
                    range = 0.01f..0.6f,
                    display = "${(state.tolerance * 100).toInt()}%",
                ) { state.tolerance = it }
                MiniSlider(
                    label = "Gap",
                    value = state.fillClose,
                    range = 0f..4f,
                    display = "${state.fillClose.toInt()}",
                ) { state.fillClose = it }
                MiniSlider(
                    label = "Grow",
                    value = state.fillExpand,
                    range = 0f..8f,
                    display = "${state.fillExpand.toInt()}",
                ) { state.fillExpand = it }
                MiniSlider(
                    label = "Soft",
                    value = state.fillFeather,
                    range = 0f..12f,
                    display = "${state.fillFeather.toInt()}",
                ) { state.fillFeather = it }
            } else {
                var brushMenu by remember { mutableStateOf(false) }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(top = 2.dp),
                ) {
                    Text("Brush", color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
                    Spacer(Modifier.weight(1f))
                    Box {
                        TextButton(
                            onClick = { brushMenu = true },
                            contentPadding = PaddingValues(horizontal = 4.dp, vertical = 0.dp),
                        ) {
                            Text(BrushPreset.fromId(state.brushKind).title, fontSize = 11.sp)
                        }
                        DropdownMenu(
                            expanded = brushMenu,
                            onDismissRequest = { brushMenu = false },
                        ) {
                            BrushPreset.entries.forEach { preset ->
                                DropdownMenuItem(
                                    text = { Text(preset.title) },
                                    onClick = {
                                        state.chooseBrush(preset.id)
                                        brushMenu = false
                                    },
                                )
                            }
                        }
                    }
                }
                MiniSlider(
                    label = "Size",
                    value = state.brushSize,
                    range = 1f..90f,
                    display = "${state.brushSize.toInt()} px",
                ) { state.brushSize = it }
                MiniSlider(
                    label = "Opacity",
                    value = state.opacity,
                    range = 0.05f..1f,
                    display = "${(state.opacity * 100).toInt()}%",
                ) { state.opacity = it }
                MirrorPicker(state)
                Row(
                    modifier = Modifier.padding(top = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("HDR", color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
                        Text("float canvas", color = Brand.Muted, fontSize = 9.sp)
                    }
                    Switch(
                        checked = state.hdr,
                        onCheckedChange = { state.chooseHdr(it) },
                    )
                }
                AnimatedVisibility(
                    visible = state.hdr,
                    enter = fadeIn(expressiveEffects()) + expandVertically(expressiveSpatial()),
                    exit = fadeOut(expressiveEffects()) + shrinkVertically(expressiveSpatial()),
                ) {
                    MiniSlider(
                        label = "Glow",
                        value = state.glow,
                        range = 0f..4f,
                        display = String.format("%.1f EV", state.glow),
                    ) { state.glow = it }
                }
                Row(
                    modifier = Modifier.padding(top = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Auto shape", color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
                        Text("tiny on-device model", color = Brand.Muted, fontSize = 9.sp)
                    }
                    Switch(
                        checked = state.autoShape,
                        onCheckedChange = { state.chooseAutoShape(it) },
                    )
                }
                Row(
                    modifier = Modifier.padding(top = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Quick shape", color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
                        Text("hold, then lift", color = Brand.Muted, fontSize = 9.sp)
                    }
                    Switch(
                        checked = state.quickShape,
                        onCheckedChange = { state.chooseQuickShape(it) },
                    )
                }
            }
            MiniSlider(
                label = "Steady",
                value = state.stabilization,
                range = 0f..0.9f,
                display = "${(state.stabilization * 100).toInt()}%",
            ) { state.updateStabilization(it) }
        }
    }
}

@Composable
fun MiniSlider(
    label: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    display: String,
    onChange: (Float) -> Unit,
) {
    Column(modifier = Modifier.padding(top = 6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(label, color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
            Spacer(Modifier.weight(1f))
            Text(display, color = Brand.Muted, fontSize = 10.sp)
        }
        Slider(value = value, onValueChange = onChange, valueRange = range, colors = brandSliderColors())
    }
}

@Composable
private fun MirrorPicker(state: PaintState) {
    Row(
        modifier = Modifier.padding(top = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("Mirror", color = Brand.Paper.copy(alpha = 0.72f), fontSize = 11.sp)
        Spacer(Modifier.weight(1f))
        listOf(0 to "Off", 1 to "H", 2 to "V", 3 to "Both").forEach { (mode, label) ->
            Surface(
                onClick = { state.chooseMirror(mode) },
                shape = RoundedCornerShape(7.dp),
                color = if (state.mirror == mode) Brand.Vermilion
                        else Color.White.copy(alpha = 0.06f),
                modifier = Modifier.padding(start = 3.dp),
            ) {
                Text(
                    label,
                    color = Brand.Paper,
                    fontSize = 9.sp,
                    modifier = Modifier.padding(horizontal = 5.dp, vertical = 4.dp),
                )
            }
        }
    }
}
