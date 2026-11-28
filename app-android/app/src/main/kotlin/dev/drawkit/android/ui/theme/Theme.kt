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

/**
 * The proof palette.
 *
 * The chrome is achromatic on purpose: a painting app's interface must never compete with the
 * picture for colour, so the room and its furniture are graphite, the text is chalk and the only
 * saturated thing in the window is the painter's own ink. Selection is paper: the tool in use is
 * coated in the current paint, not tinted with a brand hue.
 */
object Brand {
    /** The room. Cool, so the sheet is the brightest surface in the window. */
    val Graphite = Color(0xFF14171A)

    /** Chrome surfaces, one step above the room. */
    val Slate = Color(0xFF1D2126)

    /** Hairlines and dividers. */
    val Rule = Color(0xFF2E343A)

    /** Primary text and icons. */
    val Chalk = Color(0xFFECEAE4)

    /** Captions and secondary text. */
    val Ash = Color(0xFF8D959D)

    /** The document, and the fill of a selected tool. */
    val Proof = Color(0xFFF4F1EA)

    /** Destructive actions only; the rest of the chrome has no colour. */
    val Rust = Color(0xFFB4472F)
}

/** One achromatic scheme; the system palette never reaches the chrome. */
private val scheme = darkColorScheme(
    primary = Brand.Chalk,
    onPrimary = Brand.Graphite,
    primaryContainer = Brand.Proof,
    onPrimaryContainer = Brand.Graphite,
    secondary = Brand.Ash,
    onSecondary = Brand.Graphite,
    background = Brand.Graphite,
    onBackground = Brand.Chalk,
    surface = Brand.Slate,
    onSurface = Brand.Chalk,
    surfaceVariant = Brand.Rule,
    onSurfaceVariant = Brand.Ash,
    outline = Brand.Rule,
    outlineVariant = Brand.Rule,
    error = Brand.Rust,
    onError = Brand.Proof,
)

private val type = Typography(
    titleMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 15.sp,
        letterSpacing = 0.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontSize = 13.sp,
    ),
    labelSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontSize = 11.sp,
        letterSpacing = 0.2.sp,
    ),
)

/**
 * The chrome owns no colour, so the theme is a single scheme: no wallpaper tint, no light variant.
 * The document supplies the colour on screen.
 */
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
