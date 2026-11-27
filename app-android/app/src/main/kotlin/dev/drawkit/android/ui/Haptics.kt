package dev.drawkit.android.ui

import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView

/**
 * One tick for a tool or brush change.
 *
 * `performHapticFeedback` goes through the window and is dropped when the owner turned haptics off
 * in system settings, so this does not need its own switch. SEGMENT_TICK is the system's short
 * selection tick; older releases only have the keyboard one.
 */
@Composable
internal fun rememberTick(): () -> Unit {
    val view = LocalView.current
    return remember(view) {
        {
            val constant = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                HapticFeedbackConstants.SEGMENT_TICK
            } else {
                HapticFeedbackConstants.KEYBOARD_TAP
            }
            view.performHapticFeedback(constant)
        }
    }
}
