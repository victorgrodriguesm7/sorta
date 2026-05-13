package dev.sorta.tv.ui

import android.os.Bundle
import androidx.fragment.app.FragmentActivity
import dev.sorta.tv.R

/** Hosts the Leanback search fragment. Same shape as [BrowseActivity]. */
class SearchActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_search)
    }
}
