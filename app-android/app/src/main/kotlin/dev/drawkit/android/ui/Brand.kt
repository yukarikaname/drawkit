package dev.drawkit.android.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SliderColors
import androidx.compose.material3.SliderDefaults
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
fun BrandMark(size: Dp = 22.dp, line: Color = Brand.Chalk) {
    Canvas(modifier = Modifier.size(size)) {
        val s = this.size.minDimension
        val tile = s * 0.52f
        val radius = CornerRadius(s * 0.14f, s * 0.14f)
        drawRoundRect(
            color = line.copy(alpha = 0.5f),
            topLeft = Offset(s * 0.09f, s * 0.09f),
            size = Size(tile, tile),
            cornerRadius = radius,
            style = Stroke(width = s * 0.07f),
        )
        drawRoundRect(
            color = Brand.Proof,
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
            color = line,
            style = Stroke(width = s * 0.085f, cap = StrokeCap.Round),
        )
    }
}

/** Ink on a swatch: a light paint takes dark glyphs, a dark one takes chalk. */
fun inkOn(color: Color): Color {
    val luminance = 0.2126f * color.red + 0.7152f * color.green + 0.0722f * color.blue
    return if (luminance > 0.55f) Brand.Graphite else Brand.Chalk
}

/** Sliders are a custom surface, so they carry the chrome's chalk rather than a fixed brand. */
@Composable
fun brandSliderColors(): SliderColors = SliderDefaults.colors(
    thumbColor = MaterialTheme.colorScheme.onSurface,
    activeTrackColor = MaterialTheme.colorScheme.onSurface,
    inactiveTrackColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.16f),
    activeTickColor = Color.Transparent,
    inactiveTickColor = Color.Transparent,
)
