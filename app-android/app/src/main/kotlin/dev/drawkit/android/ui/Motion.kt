@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

// Motion comes from the Material 3 Expressive scheme. Components ask the theme instead of
// carrying their own spring numbers, so a future theme change moves every panel together.

package dev.drawkit.android.ui

import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable

/** Spatial motion: position, size and scale. */
@Composable
internal fun <T> expressiveSpatial(): FiniteAnimationSpec<T> = MaterialTheme.motionScheme.defaultSpatialSpec()

/** Fast spatial motion for a selection that should answer immediately. */
@Composable
internal fun <T> expressiveFast(): FiniteAnimationSpec<T> = MaterialTheme.motionScheme.fastSpatialSpec()

/** Effects motion: colour, alpha and elevation. */
@Composable
internal fun <T> expressiveEffects(): FiniteAnimationSpec<T> = MaterialTheme.motionScheme.defaultEffectsSpec()
