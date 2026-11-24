package dev.drawkit.android.ui

import androidx.compose.foundation.Canvas
import androidx.compose.material3.SliderColors
import androidx.compose.material3.SliderDefaults
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.drawkit.android.ui.theme.Brand

/** Two tiles and a brush stroke: the mark used in the window, the sheets and the icon. */
@Composable
fun BrandMark(size: Dp = 22.dp, line: Color = Brand.Paper) {
    Canvas(modifier = Modifier.size(size)) {
        val s = this.size.minDimension
        val tile = s * 0.52f
        val radius = CornerRadius(s * 0.14f, s * 0.14f)
        drawRoundRect(
            color = line.copy(alpha = 0.55f),
            topLeft = Offset(s * 0.09f, s * 0.09f),
            size = Size(tile, tile),
            cornerRadius = radius,
            style = Stroke(width = s * 0.07f),
        )
        drawRoundRect(
            color = Brand.Vermilion,
            topLeft = Offset(s * 0.39f, s * 0.39f),
            size = Size(tile, tile),
            cornerRadius = radius,
        )
        val stroke = Path().apply {
            moveTo(s * 0.20f, s * 0.80f)
            quadraticTo(s * 0.56f, s * 0.56f, s * 0.80f, s * 0.20f)
        }
        drawPath(
            path = stroke,
            color = Brand.Paper,
            style = Stroke(width = s * 0.085f, cap = StrokeCap.Round),
        )
    }
}

/** Sliders are a custom surface, so they carry the vermilion rather than the theme's default. */
@Composable
fun brandSliderColors(): SliderColors = SliderDefaults.colors(
    thumbColor = Brand.Vermilion,
    activeTrackColor = Brand.Vermilion,
    inactiveTrackColor = Brand.Paper.copy(alpha = 0.16f),
    activeTickColor = Color.Transparent,
    inactiveTickColor = Color.Transparent,
)
