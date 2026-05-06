package dev.sorta.tv

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import android.widget.TextView

/**
 * Placeholder root activity. Real Leanback BrowseSupportFragment-based
 * UI lands in a later commit — this just gives the manifest something
 * to point at so the project builds + sideloads.
 */
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val text = TextView(this).apply {
            text = getString(R.string.app_name) + " — booting"
            textSize = 24f
            setPadding(48, 48, 48, 48)
        }
        setContentView(text)
    }
}
