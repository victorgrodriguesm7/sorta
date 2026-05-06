package dev.sorta.tv.ui

import android.os.Bundle
import androidx.leanback.app.BrowseSupportFragment
import androidx.leanback.widget.ArrayObjectAdapter
import androidx.leanback.widget.ListRowPresenter
import dev.sorta.tv.R

/**
 * Skeleton Leanback browse fragment. Sets up the standard "rows of
 * cards" layout but starts with an empty adapter — real rows are
 * wired up in a later commit once MediaRepository is plumbed in.
 */
class BrowseFragment : BrowseSupportFragment() {

    private lateinit var rowsAdapter: ArrayObjectAdapter

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = getString(R.string.app_name)
        // Hide the headers strip until we have rows to populate it
        // with — avoids a flash of empty UI on first launch.
        headersState = HEADERS_DISABLED
        isHeadersTransitionOnBackEnabled = false
        rowsAdapter = ArrayObjectAdapter(ListRowPresenter())
        adapter = rowsAdapter
    }
}
