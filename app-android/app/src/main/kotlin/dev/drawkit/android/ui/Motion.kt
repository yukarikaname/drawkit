// The Compose side of the shared motion tokens. The numbers match Motion.swift on macOS: a
// selection is quicker than a panel, and a panel settles with a little more weight.

package dev.drawkit.android.ui

import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Dp

internal object Motion {
    val select: FiniteAnimationSpec<Float> =
        spring(dampingRatio = 0.74f, stiffness = 420f)
    val settle: FiniteAnimationSpec<Float> =
        spring(dampingRatio = Spring.DampingRatioNoBouncy, stiffness = 260f)
    val press: FiniteAnimationSpec<Float> = tween(durationMillis = 120)
    val panelSize: FiniteAnimationSpec<IntSize> =
        spring(dampingRatio = 0.86f, stiffness = 240f)
    val fade: FiniteAnimationSpec<Float> = tween(durationMillis = 160)
    val colour: FiniteAnimationSpec<Color> =
        spring(dampingRatio = 0.80f, stiffness = 260f)
    val offset: FiniteAnimationSpec<androidx.compose.ui.unit.IntOffset> =
        spring(dampingRatio = 0.86f, stiffness = 300f)
    val elevation: FiniteAnimationSpec<Dp> =
        spring(dampingRatio = 0.80f, stiffness = 200f)
}
