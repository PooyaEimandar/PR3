package ai.pooya.pr3

import android.graphics.Color
import android.os.Bundle
import android.view.View
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // PR3 is dark whatever the system's theme, so the bars' icons stay light.
    enableEdgeToEdge(SystemBarStyle.dark(Color.TRANSPARENT), SystemBarStyle.dark(Color.TRANSPARENT))
    super.onCreate(savedInstanceState)
    // Android 15 draws apps edge to edge and no longer resizes them for the keyboard, and
    // the webview reports no safe-area insets, so the page sits between the status bar and
    // the navigation bar or the keyboard, whichever is taller. Otherwise the window slides
    // up under the status bar while typing. Behind the bars is the page's background.
    val content = findViewById<View>(android.R.id.content)
    content.setBackgroundColor(Color.rgb(0x05, 0x08, 0x05))
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, keyboard.bottom))
      WindowInsetsCompat.CONSUMED
    }
  }
}
