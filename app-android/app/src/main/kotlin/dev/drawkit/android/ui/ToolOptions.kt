package dev.drawkit.android.ui

import android.os.Build
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
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

@Composable
internal fun ToolOptions(
    state: PaintState,
    onImportBrush: () -> Unit,
    onExportBrush: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tick = rememberTick()
    Surface(
        modifier = modifier.width(176.dp),
        shape = RoundedCornerShape(18.dp),
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.9f),
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
                        state.tool == Tool.Select
                    ) {
                        "tap"
                    } else {
                        "${state.activeSize.toInt()} px"
                    },
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    fontSize = 11.sp,
                )
            }
            ToolSection(visible = state.tool == Tool.Select) {
                Text(
                    "Drag a rectangle on the canvas",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    fontSize = 11.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
                Row(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.padding(top = 6.dp),
                ) {
                    Button(onClick = { state.extractSelection() }) { Text("Extract") }
                    Button(onClick = { state.chooseSelection(null) }) { Text("Deselect") }
                    Button(onClick = { state.clearLayer() }) { Text("Clear") }
                }
            }
            ToolSection(visible = state.tool == Tool.Eyedropper) {
                Text(
                    "Tap the canvas to sample a colour",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    fontSize = 11.sp,
                    modifier = Modifier.padding(top = 8.dp, bottom = 4.dp),
                )
            }
            ToolSection(visible = state.tool == Tool.Fill) {
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
                Row(
                    modifier = Modifier.padding(top = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            "Snap colour",
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
                            fontSize = 11.sp,
                        )
                        Text(
                            "keep a close refill colour",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            fontSize = 9.sp,
                        )
                    }
                    Switch(
                        checked = state.fillSnap,
                        onCheckedChange = { state.chooseFillSnap(it) },
                    )
                }
            }
            ToolSection(visible = state.tool == Tool.Brush || state.tool == Tool.Eraser) {
                var brushMenu by remember { mutableStateOf(false) }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(top = 2.dp),
                ) {
                    Text("Brush", color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f), fontSize = 11.sp)
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
                                        tick()
                                        state.chooseBrush(preset.id)
                                        brushMenu = false
                                    },
                                )
                            }
                            DropdownMenuItem(text = { Text("Import brush…") }, onClick = {
                                brushMenu = false
                                onImportBrush()
                            })
                            DropdownMenuItem(text = { Text("Export brush…") }, onClick = {
                                brushMenu = false
                                onExportBrush()
                            })
                        }
                    }
                }
                MiniSlider(
                    label = "Size",
                    value = state.activeSize,
                    range = 1f..90f,
                    display = "${state.activeSize.toInt()} px",
                ) {
                    if (state.tool == Tool.Eraser) state.eraserSize = it else state.brushSize = it
                }
                MiniSlider(
                    label = "Opacity",
                    value = state.activeOpacity,
                    range = 0.05f..1f,
                    display = "${(state.activeOpacity * 100).toInt()}%",
                ) {
                    if (state.tool == Tool.Eraser) state.chooseEraserOpacity(it) else state.opacity = it
                }
                MirrorPicker(state)
                Row(
                    modifier = Modifier.padding(top = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            "HDR",
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
                            fontSize = 11.sp,
                        )
                        Text(
                            "float canvas",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            fontSize = 9.sp,
                        )
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
                        Text(
                            "Auto shape",
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
                            fontSize = 11.sp,
                        )
                        Text(
                            "tiny on-device model",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            fontSize = 9.sp,
                        )
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
                        Text(
                            "Quick shape",
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
                            fontSize = 11.sp,
                        )
                        Text(
                            "hold, then lift",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            fontSize = 9.sp,
                        )
                    }
                    Switch(
                        checked = state.quickShape,
                        onCheckedChange = { state.chooseQuickShape(it) },
                    )
                }
            }
            // Material You recolours the chrome, not the canvas. Older releases have no dynamic
            // palette, so the switch would promise something they cannot deliver.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                Row(
                    modifier = Modifier.padding(top = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            "Chrome",
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
                            fontSize = 11.sp,
                        )
                        Text(
                            if (state.chromeFollowsSystem) "system palette" else "DrawKit palette",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            fontSize = 9.sp,
                        )
                    }
                    Switch(
                        checked = state.chromeFollowsSystem,
                        onCheckedChange = { state.chooseChrome(it) },
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

/** The controls a tool owns, shown and hidden with the tool itself. */
@Composable
private fun ToolSection(visible: Boolean, content: @Composable ColumnScope.() -> Unit) {
    AnimatedVisibility(
        visible = visible,
        enter = fadeIn(expressiveEffects()) + expandVertically(expressiveSpatial()),
        exit = fadeOut(expressiveEffects()) + shrinkVertically(expressiveSpatial()),
    ) {
        Column(content = content)
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
            Text(label, color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f), fontSize = 11.sp)
            Spacer(Modifier.weight(1f))
            Text(display, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 10.sp)
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
        Text("Mirror", color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f), fontSize = 11.sp)
        Spacer(Modifier.weight(1f))
        listOf(0 to "Off", 1 to "H", 2 to "V", 3 to "Both").forEach { (mode, label) ->
            val selected = state.mirror == mode
            val fill by animateColorAsState(
                if (selected) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.onSurface.copy(alpha = 0.06f)
                },
                expressiveEffects(),
                label = "mirror",
            )
            val ink by animateColorAsState(
                if (selected) {
                    MaterialTheme.colorScheme.onPrimary
                } else {
                    MaterialTheme.colorScheme.onSurface
                },
                expressiveEffects(),
                label = "mirrorInk",
            )
            Surface(
                onClick = { state.chooseMirror(mode) },
                shape = RoundedCornerShape(7.dp),
                color = fill,
                modifier = Modifier.padding(start = 3.dp),
            ) {
                Text(
                    label,
                    color = ink,
                    fontSize = 9.sp,
                    modifier = Modifier.padding(horizontal = 5.dp, vertical = 4.dp),
                )
            }
        }
    }
}
