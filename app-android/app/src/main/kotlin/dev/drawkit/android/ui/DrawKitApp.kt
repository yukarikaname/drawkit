package dev.drawkit.android.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.border
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Redo
import androidx.compose.material.icons.automirrored.rounded.Undo
import androidx.compose.material.icons.rounded.Brush
import androidx.compose.material.icons.rounded.CleaningServices
import androidx.compose.material.icons.rounded.Colorize
import androidx.compose.material.icons.rounded.CropFree
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.TextButton
import androidx.compose.material.icons.rounded.FormatColorFill
import androidx.compose.material.icons.rounded.Layers
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import dev.drawkit.android.core.BrushPreset
import dev.drawkit.android.core.PaintState
import dev.drawkit.android.core.Tool
import dev.drawkit.android.ui.theme.Brand

@Composable
fun DrawKitApp(state: PaintState = remember { PaintState() }) {
    DisposableEffect(Unit) {
        onDispose { state.release() }
    }

    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }

    val wide = LocalConfiguration.current.screenWidthDp >= 900
    BackHandler(enabled = state.showLayers || state.showColours) {
        state.showLayers = false
        state.showColours = false
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(Brand.Ink)
            .focusRequester(focus)
            .focusable()
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                if (event.isCtrlPressed) {
                    when (event.key) {
                        Key.Z -> { state.undo(); true }
                        Key.Y -> { state.redo(); true }
                        else -> false
                    }
                } else {
                    when (event.key) {
                        Key.B -> { state.tool = Tool.Brush; true }
                        Key.E -> { state.tool = Tool.Eraser; true }
                        Key.F -> { state.tool = Tool.Fill; true }
                        else -> false
                    }
                }
            },
    ) {
        AndroidView(
            factory = { context ->
                DrawingView(context).also { view ->
                    view.state = state
                    state.onInvalidate = { view.invalidate() }
                }
            },
            modifier = Modifier.fillMaxSize(),
            update = { view ->
                view.state = state
                state.onInvalidate = { view.invalidate() }
            },
        )

        TopBar(
            state = state,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .statusBarsPadding()
                .padding(horizontal = 10.dp, vertical = 10.dp),
        )

        if (!state.isBlank) {
            ToolOptions(
                state = state,
                modifier = Modifier.align(Alignment.CenterStart).padding(start = 10.dp),
            )
        }

        if (state.isBlank) {
            StartHint(modifier = Modifier.align(Alignment.Center).padding(horizontal = 28.dp))
        }

        StatusLine(
            state = state,
            modifier = Modifier
                .align(Alignment.BottomStart)
                .navigationBarsPadding()
                .padding(start = 18.dp, bottom = 14.dp),
        )

        ColourWell(
            state = state,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .navigationBarsPadding()
                .padding(end = 18.dp, bottom = 14.dp),
        )

        if (wide && state.showLayers) {
            LayerDock(
                state,
                Modifier.align(Alignment.CenterEnd).padding(end = 12.dp).width(300.dp),
            )
        }
    }

    if (state.showLayers && !wide) {
        LayersSheet(state = state, onDismiss = { state.showLayers = false })
    }
    if (state.showColours) {
        ColourSheet(state = state, onDismiss = { state.showColours = false })
    }
}

@Composable
private fun TopBar(state: PaintState, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        color = Brand.Carbon.copy(alpha = 0.94f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.07f)),
        shadowElevation = 12.dp,
    ) {
        Row(
            modifier = Modifier.height(56.dp).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(modifier = Modifier.padding(start = 4.dp), contentAlignment = Alignment.Center) {
                BrandMark(size = 24.dp)
            }
            Spacer(Modifier.width(10.dp))
            ToolGlyph(Icons.Rounded.Brush, "Brush", state.tool == Tool.Brush) {
                state.tool = Tool.Brush
            }
            ToolGlyph(Icons.Rounded.CleaningServices, "Eraser", state.tool == Tool.Eraser) {
                state.tool = Tool.Eraser
            }
            ToolGlyph(Icons.Rounded.FormatColorFill, "Fill", state.tool == Tool.Fill) {
                state.tool = Tool.Fill
            }
            ToolGlyph(Icons.Rounded.Colorize, "Eyedropper", state.tool == Tool.Eyedropper) {
                state.tool = Tool.Eyedropper
            }
            ToolGlyph(Icons.Rounded.CropFree, "Select", state.tool == Tool.Select) {
                state.tool = Tool.Select
            }
            Spacer(Modifier.weight(1f))
            Glyph(Icons.AutoMirrored.Rounded.Undo, "Undo", enabled = state.canUndo) {
                state.undo()
            }
            Glyph(Icons.AutoMirrored.Rounded.Redo, "Redo", enabled = state.canRedo) {
                state.redo()
            }
            Glyph(Icons.Rounded.Layers, "Layers") { state.toggleLayers() }
        }
    }
}

@Composable
private fun ToolGlyph(icon: ImageVector, description: String, selected: Boolean, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(44.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(if (selected) Brand.Vermilion else Color.Transparent)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = description,
            tint = if (selected) Brand.Paper else Brand.Paper.copy(alpha = 0.72f),
            modifier = Modifier.size(22.dp),
        )
    }
}

@Composable
fun Glyph(
    icon: ImageVector,
    description: String,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(44.dp)
            .clip(RoundedCornerShape(12.dp))
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = description,
            tint = Brand.Paper.copy(alpha = if (enabled) 0.78f else 0.22f),
            modifier = Modifier.size(22.dp),
        )
    }
}

@Composable
private fun StartHint(modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(20.dp),
        color = Brand.Carbon.copy(alpha = 0.92f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.08f)),
        shadowElevation = 16.dp,
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 30.dp, vertical = 26.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            BrandMark(size = 52.dp)
            Text(
                "Start with a line.",
                style = MaterialTheme.typography.headlineSmall,
                color = Brand.Paper,
                textAlign = TextAlign.Center,
            )
            Text(
                "Brush · Eraser · Fill in the top bar",
                color = Brand.Paper.copy(alpha = 0.55f),
                fontSize = 12.sp,
            )
        }
    }
}

@Composable
private fun StatusLine(state: PaintState, modifier: Modifier = Modifier) {
    Row(modifier = modifier, verticalAlignment = Alignment.Bottom) {
        Column {
            Text(
                "DrawKit",
                color = Brand.Paper,
                fontWeight = FontWeight.SemiBold,
                fontSize = 13.sp,
                letterSpacing = 0.6.sp,
            )
            Text(
                "${state.controller.width} × ${state.controller.height} · ${state.layers.size} " +
                    if (state.layers.size == 1) "layer" else "layers",
                color = Brand.Muted,
                fontSize = 10.sp,
            )
        }
    }
}

@Composable
private fun ColourWell(state: PaintState, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .size(56.dp)
            .shadow(10.dp, CircleShape)
            .clip(CircleShape)
            .background(state.color)
            .border(2.dp, Color.White.copy(alpha = 0.35f), CircleShape)
            .clickable { state.toggleColours() },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .size(56.dp)
                .clip(CircleShape)
                .background(Color.Transparent),
        )
    }
}
