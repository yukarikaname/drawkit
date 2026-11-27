package dev.drawkit.android.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import dev.drawkit.android.core.PaintState
import dev.drawkit.android.core.hslToColor
import kotlin.math.roundToInt

private enum class ColourTab { Grid, Sliders }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ColourSheet(state: PaintState, onDismiss: () -> Unit) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var tab by remember { mutableStateOf(ColourTab.Grid) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surface,
        contentColor = MaterialTheme.colorScheme.onSurface,
        dragHandle = { BottomSheetDefaults.DragHandle(color = MaterialTheme.colorScheme.onSurfaceVariant) },
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 18.dp).padding(bottom = 22.dp)
                .navigationBarsPadding().safeHorizontalInsets(),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                BrandMark(size = 26.dp)
                Spacer(Modifier.width(10.dp))
                Text("Colours", style = androidx.compose.material3.MaterialTheme.typography.headlineSmall)
                Spacer(Modifier.weight(1f))
                Button(onClick = onDismiss, shape = CircleShape) { Text("Done") }
            }

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TabPill("Grid", tab == ColourTab.Grid) { tab = ColourTab.Grid }
                TabPill("Sliders", tab == ColourTab.Sliders) { tab = ColourTab.Sliders }
            }

            when (tab) {
                ColourTab.Grid -> ColourGrid(state, Modifier.fillMaxWidth())
                ColourTab.Sliders -> ColourSliders(state)
            }

            Footer(state)
        }
    }
}

@Composable
private fun TabPill(label: String, selected: Boolean, onClick: () -> Unit) {
    Surface(
        shape = CircleShape,
        color = if (selected) MaterialTheme.colorScheme.surfaceVariant else Color.Transparent,
        onClick = onClick,
    ) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
            color = if (selected) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
            fontSize = 13.sp,
        )
    }
}

@Composable
private fun ColourGrid(state: PaintState, modifier: Modifier = Modifier) {
    val columns = 12
    val rows = 7
    val hsv = remember(state.color) { hsvOf(state.color) }
    val cell = Modifier
        .aspectRatio(columns.toFloat() / (rows + 1))
        .clip(RoundedCornerShape(12.dp))
        .pointerInput(Unit) {
            detectDragGestures(
                onDragStart = { pick(state, it, size.width.toFloat(), size.height.toFloat(), columns, rows) },
                onDrag = { change, _ ->
                    pick(state, change.position, size.width.toFloat(), size.height.toFloat(), columns, rows)
                },
            )
        }
        .pointerInput(Unit) {
            detectTapGestures {
                pick(state, it, size.width.toFloat(), size.height.toFloat(), columns, rows)
            }
        }

    Canvas(modifier = modifier.then(cell)) {
        val cw = size.width / columns
        val ch = size.height / (rows + 1)
        for (column in 0 until columns) {
            val grey = 1f - column / (columns - 1f)
            drawRect(
                color = Color(grey, grey, grey, 1f),
                topLeft = Offset(column * cw, 0f),
                size = Size(cw, ch),
            )
        }
        for (row in 0 until rows) {
            for (column in 0 until columns) {
                val hue = (column + 0.5f) / columns
                val lightness = 0.16f + row / (rows - 1f) * 0.74f
                drawRect(
                    color = hslToColor(hue, 1f, lightness),
                    topLeft = Offset(column * cw, (row + 1) * ch),
                    size = Size(cw + 0.5f, ch + 0.5f),
                )
            }
        }
        val selected = selectedCell(hsv, columns, rows)
        drawRoundRect(
            color = Color.White,
            topLeft = Offset(selected.first * cw + 2f, selected.second * ch + 2f),
            size = Size(cw - 4f, ch - 4f),
            cornerRadius = CornerRadius(3f, 3f),
            style = Stroke(width = 2.5f),
        )
    }
}

private fun pick(
    state: PaintState, point: Offset, width: Float, height: Float, columns: Int, rows: Int,
) {
    if (width <= 0f || height <= 0f) return
    val column = (point.x / (width / columns)).toInt().coerceIn(0, columns - 1)
    val row = (point.y / (height / (rows + 1))).toInt().coerceIn(0, rows)
    state.color = if (row == 0) {
        val grey = 1f - column / (columns - 1f)
        Color(grey, grey, grey, 1f)
    } else {
        hslToColor((column + 0.5f) / columns, 1f, 0.16f + (row - 1) / (rows - 1f) * 0.74f)
    }
    state.rememberColour()
}

private fun selectedCell(hsv: FloatArray, columns: Int, rows: Int): Pair<Int, Int> {
    val hue = hsv[0] / 360f
    val saturation = hsv[1]
    val value = hsv[2]
    if (saturation < 0.10f) {
        val column = ((1f - value) * (columns - 1)).roundToInt().coerceIn(0, columns - 1)
        return column to 0
    }
    val column = (hue * columns).toInt().coerceIn(0, columns - 1)
    val lightness = value * (1f - saturation / 2f)
    val row = ((lightness - 0.16f) / 0.74f * (rows - 1)).roundToInt().coerceIn(0, rows - 1)
    return column to row + 1
}

@Composable
private fun ColourSliders(state: PaintState) {
    val hsv = hsvOf(state.color)
    val alpha = state.color.alpha
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        SliderLine("Hue", hsv[0], 0f..360f, "${hsv[0].toInt()}°") {
            state.color = colorOf(it, hsv[1], hsv[2], alpha)
        }
        SliderLine("Saturation", hsv[1], 0f..1f, "${(hsv[1] * 100).toInt()}%") {
            state.color = colorOf(hsv[0], it, hsv[2], alpha)
        }
        SliderLine("Brightness", hsv[2], 0f..1f, "${(hsv[2] * 100).toInt()}%") {
            state.color = colorOf(hsv[0], hsv[1], it, alpha)
        }
        SliderLine("Opacity", alpha, 0.05f..1f, "${(alpha * 100).toInt()}%") {
            state.color = state.color.copy(alpha = it)
        }
    }
}

@Composable
private fun SliderLine(
    label: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    display: String,
    onChange: (Float) -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp, modifier = Modifier.width(86.dp))
        Slider(
            value = value,
            onValueChange = onChange,
            valueRange = range,
            modifier = Modifier.weight(1f),
            colors = brandSliderColors(),
        )
        Text(display, color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f), fontSize = 11.sp, modifier = Modifier.width(44.dp))
    }
}

@Composable
private fun Footer(state: PaintState) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
        Box(
            modifier = Modifier.size(32.dp).clip(CircleShape).background(state.color),
        )
        CircleAction(Icons.Rounded.Add, "Keep this colour") { state.rememberColour() }
        state.recent.take(7).forEach { swatch ->
            Box(
                modifier = Modifier
                    .size(26.dp)
                    .clip(CircleShape)
                    .background(swatch)
                    .clickable { state.color = swatch },
            )
        }
        Spacer(Modifier.weight(1f))
        CircleAction(Icons.Rounded.AutoAwesome, "Sample the artwork") { state.sampleArtwork() }
    }
}

@Composable
private fun CircleAction(icon: androidx.compose.ui.graphics.vector.ImageVector, description: String, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(30.dp)
            .clip(CircleShape)
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, description, tint = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.8f), modifier = Modifier.size(16.dp))
    }
}

private fun hsvOf(color: Color): FloatArray =
    FloatArray(3).also { android.graphics.Color.colorToHSV(color.toArgb(), it) }

private fun colorOf(hue: Float, saturation: Float, value: Float, alpha: Float): Color {
    val argb = android.graphics.Color.HSVToColor(
        (alpha * 255f).roundToInt().coerceIn(0, 255),
        floatArrayOf(hue, saturation, value),
    )
    return Color(argb)
}
