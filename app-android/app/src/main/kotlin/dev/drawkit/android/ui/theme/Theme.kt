package dev.drawkit.android.ui.theme

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

/** The same identity as the macOS shell: warm ink, warm paper and one vermilion. */
object Brand {
    val Ink = Color(0xFF1B1917)
    val Carbon = Color(0xFF262320)
    val Raised = Color(0xFF332F2B)
    val Vermilion = Color(0xFFE2563E)
    val VermilionDeep = Color(0xFFC73E29)
    val Paper = Color(0xFFF6F1E9)
    val Muted = Color(0xFF9C948A)
}

private val scheme = darkColorScheme(
    primary = Brand.Vermilion,
    onPrimary = Brand.Paper,
    primaryContainer = Brand.VermilionDeep,
    onPrimaryContainer = Brand.Paper,
    background = Brand.Ink,
    onBackground = Brand.Paper,
    surface = Brand.Carbon,
    onSurface = Brand.Paper,
    surfaceVariant = Brand.Raised,
    onSurfaceVariant = Brand.Muted,
    outline = Color(0xFF57514A),
    secondary = Brand.Vermilion,
    onSecondary = Brand.Paper,
)

private val type = Typography(
    headlineSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Bold,
        fontSize = 24.sp,
    ),
    titleMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 16.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontSize = 14.sp,
    ),
    labelSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontSize = 11.sp,
    ),
)

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun DrawKitTheme(content: @Composable () -> Unit) {
    MaterialExpressiveTheme(
        colorScheme = scheme,
        typography = type,
        motionScheme = MotionScheme.expressive(),
        content = content,
    )
}
