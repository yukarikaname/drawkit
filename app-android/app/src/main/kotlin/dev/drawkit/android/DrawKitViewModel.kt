package dev.drawkit.android

import androidx.lifecycle.ViewModel
import dev.drawkit.android.core.PaintState

/**
 * The document outlives the window. A desktop window is resized, a tablet is folded, the theme
 * changes — each of those re-creates the activity, and the resident tiles go with it. The state
 * holds the painting core and its CPU cache, so the new window re-uploads rather than restarting.
 */
class DrawKitViewModel : ViewModel() {
    val state = PaintState()

    override fun onCleared() {
        state.close()
    }
}
