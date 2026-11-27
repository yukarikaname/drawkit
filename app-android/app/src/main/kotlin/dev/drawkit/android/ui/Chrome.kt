package dev.drawkit.android.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Redo
import androidx.compose.material.icons.automirrored.rounded.Undo
import androidx.compose.material.icons.rounded.Brush
import androidx.compose.material.icons.rounded.CleaningServices
import androidx.compose.material.icons.rounded.Colorize
import androidx.compose.material.icons.rounded.CropFree
import androidx.compose.material.icons.rounded.FormatColorFill
import androidx.compose.material.icons.rounded.Layers
import androidx.compose.material.icons.rounded.Movie
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import dev.drawkit.android.core.PaintState
import dev.drawkit.android.core.Tool

/**
 * The tools as a vertical rail, for a window wide enough to give the canvas its own column.
 *
 * On a desktop window the horizontal bar costs a strip of canvas height and the parameter panel
 * would float over the painting. A rail keeps the tools at thumb distance on a tablet and at
 * pointer distance on a laptop, and never covers a document pixel.
 */
@Composable
internal fun ToolRail(state: PaintState, modifier: Modifier = Modifier) {
    val tick = rememberTick()
    Surface(
        modifier = modifier.width(56.dp).fillMaxHeight(),
        shape = RoundedCornerShape(20.dp),
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.94f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.07f)),
        shadowElevation = 12.dp,
    ) {
        Column(
            modifier = Modifier.padding(vertical = 8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            BrandMark(size = 24.dp)
            Spacer(Modifier.height(6.dp))
            ToolGlyph(Icons.Rounded.Brush, "Brush", state.tool == Tool.Brush) {
                tick()
                state.tool = Tool.Brush
            }
            ToolGlyph(Icons.Rounded.CleaningServices, "Eraser", state.tool == Tool.Eraser) {
                tick()
                state.tool = Tool.Eraser
            }
            ToolGlyph(Icons.Rounded.FormatColorFill, "Fill", state.tool == Tool.Fill) {
                tick()
                state.tool = Tool.Fill
            }
            ToolGlyph(Icons.Rounded.Colorize, "Eyedropper", state.tool == Tool.Eyedropper) {
                tick()
                state.tool = Tool.Eyedropper
            }
            ToolGlyph(Icons.Rounded.CropFree, "Select", state.tool == Tool.Select) {
                tick()
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
            Glyph(Icons.Rounded.Movie, "Animation") { state.showAnimation = !state.showAnimation }
        }
    }
}

@Composable
internal fun ToolGlyph(icon: ImageVector, description: String, selected: Boolean, onClick: () -> Unit) {
    val background by animateColorAsState(
        if (selected) MaterialTheme.colorScheme.primary else Color.Transparent,
        expressiveEffects(),
        label = "tool",
    )
    val grow by animateFloatAsState(
        if (selected) 1.06f else 1f,
        expressiveFast(),
        label = "toolScale",
    )
    Box(
        modifier = Modifier
            .size(44.dp)
            .scale(grow)
            .clip(RoundedCornerShape(12.dp))
            .background(background)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = description,
            tint = if (selected) {
                MaterialTheme.colorScheme.onPrimary
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f)
            },
            modifier = Modifier.size(22.dp),
        )
    }
}

@Composable
internal fun Glyph(icon: ImageVector, description: String, enabled: Boolean = true, onClick: () -> Unit) {
    // Undo and redo fade between available and unavailable instead of blinking.
    val tint by animateColorAsState(
        MaterialTheme.colorScheme.onSurface.copy(alpha = if (enabled) 0.78f else 0.22f),
        expressiveEffects(),
        label = "glyph",
    )
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
            tint = tint,
            modifier = Modifier.size(22.dp),
        )
    }
}
