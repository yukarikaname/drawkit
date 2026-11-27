package dev.drawkit.android.ui

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/**
 * The left and right safe insets only.
 *
 * A landscape tablet can have a camera cutout or a navigation rail on either side, and the chrome
 * that hugs an edge would sit under it. Vertical insets are already handled by each bar's own
 * `statusBarsPadding`/`navigationBarsPadding`, so taking them again here would double the gap.
 */
@Composable
internal fun Modifier.safeHorizontalInsets(): Modifier =
    windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))

/** Status bar, display cutout and the two landscape sides, but not the navigation bar below. */
@Composable
internal fun Modifier.safeTopInsets(): Modifier = windowInsetsPadding(
    WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal),
)
