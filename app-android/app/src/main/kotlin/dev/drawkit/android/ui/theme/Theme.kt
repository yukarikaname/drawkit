package dev.drawkit.android.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
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

/** The brand studio: a dark room, because the picture is the only bright thing in it. */
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

/**
 * The chrome may follow Material You; the canvas must not.
 *
 * A painting app's working surface is part of the picture: the paper, the surround, the guides and
 * the HDR preview keep the brand tones even when the tool bar, the sheets and the buttons take the
 * system palette. Recolouring the canvas by wallpaper would change what the artist sees while
 * judging colour, so only chrome composables read [MaterialExpressiveTheme]'s scheme.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun DrawKitTheme(followSystem: Boolean = true, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val dark = isSystemInDarkTheme()
    val dynamic = followSystem && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
    val colors = when {
        dynamic && dark -> dynamicDarkColorScheme(context)
        dynamic -> dynamicLightColorScheme(context)
        else -> scheme
    }
    MaterialExpressiveTheme(
        colorScheme = colors,
        typography = type,
        motionScheme = MotionScheme.expressive(),
        content = content,
    )
}
