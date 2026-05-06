package dev.sorta.tv

import android.content.Intent
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import dev.sorta.tv.ui.BrowseActivity

/**
 * Thin entry point. For now we always route to [BrowseActivity];
 * first-run drive-selection logic lands in Phase 5.
 */
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        startActivity(Intent(this, BrowseActivity::class.java))
        finish()
    }
}
