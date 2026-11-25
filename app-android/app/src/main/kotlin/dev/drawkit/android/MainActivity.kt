package dev.drawkit.android

import android.content.pm.ActivityInfo
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import dev.drawkit.android.ui.DrawKitApp
import dev.drawkit.android.ui.theme.DrawKitTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        if (Build.VERSION.SDK_INT >= 34) {
            // Float bitmaps keep values above paper white. Request an HDR window so the system
            // presents them as light instead of clipping them; SDR documents still look SDR.
            runCatching { window.colorMode = ActivityInfo.COLOR_MODE_HDR }
        }
        setContent {
            DrawKitTheme {
                DrawKitApp()
            }
        }
    }
}
