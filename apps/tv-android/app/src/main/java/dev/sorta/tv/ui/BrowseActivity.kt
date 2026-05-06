package dev.sorta.tv.ui

import android.os.Bundle
import androidx.fragment.app.FragmentActivity
import dev.sorta.tv.R

/**
 * Hosts the Leanback [BrowseFragment]. Kept thin — all the UI lives
 * in the fragment so the activity is just the FragmentManager owner.
 */
class BrowseActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_browse)
    }
}
