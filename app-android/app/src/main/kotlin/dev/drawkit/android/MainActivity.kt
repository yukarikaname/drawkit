package dev.drawkit.android

import android.content.pm.ActivityInfo
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import dev.drawkit.android.ui.DrawKitApp
import dev.drawkit.android.ui.theme.DrawKitTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        super.onCreate(savedInstanceState)
        // The canvas paints to the screen edge; only the chrome takes the system bar insets.
        enableEdgeToEdge()
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        if (Build.VERSION.SDK_INT >= 34) {
            // Float bitmaps keep values above paper white. Request an HDR window so the system
            // presents them as light instead of clipping them; SDR documents still look SDR.
            runCatching { window.colorMode = ActivityInfo.COLOR_MODE_HDR }
        }
        val model: DrawKitViewModel by viewModels()
        setContent {
            // The state owns the chrome setting, so the theme and the panel read the same value.
            val state = model.state
            val override = remember { debugLayoutDirection() }
            val direction = override ?: LocalLayoutDirection.current
            CompositionLocalProvider(LocalLayoutDirection provides direction) {
                DrawKitTheme(followSystem = state.chromeFollowsSystem) {
                    DrawKitApp(state)
                }
            }
        }
    }

    /**
     * A debugging override for the layout direction.
     *
     * `setprop` writes an Android property, not a Java one, so the intent extra is the route that
     * works on a device:
     *
     *     adb shell am start -n dev.drawkit.android/.MainActivity \
     *         --es debug.drawkit.layoutDirection rtl
     *
     * `System.getProperty` is still read first for hosts that pass `-D` to the runtime (an
     * emulator launcher or a test harness); it costs nothing and keeps the pen in one place.
     */
    private fun debugLayoutDirection(): LayoutDirection? {
        val requested = intent?.getStringExtra(EXTRA_LAYOUT_DIRECTION)
            ?: System.getProperty("debug.drawkit.layoutDirection")
        return when (requested?.lowercase()) {
            "rtl" -> LayoutDirection.Rtl
            "ltr" -> LayoutDirection.Ltr
            else -> null
        }
    }

    companion object {
        const val EXTRA_LAYOUT_DIRECTION = "debug.drawkit.layoutDirection"
    }
}
