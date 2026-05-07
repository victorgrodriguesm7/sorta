package dev.sorta.tv.ui

import android.os.Bundle
import android.widget.Toast
import androidx.leanback.app.SearchSupportFragment
import androidx.leanback.widget.ArrayObjectAdapter
import androidx.leanback.widget.HeaderItem
import androidx.leanback.widget.ListRow
import androidx.leanback.widget.ListRowPresenter
import androidx.leanback.widget.ObjectAdapter
import androidx.leanback.widget.OnItemViewClickedListener
import androidx.lifecycle.lifecycleScope
import dev.sorta.tv.R
import dev.sorta.tv.data.MediaRepository
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import dev.sorta.tv.data.WatchHistory
import dev.sorta.tv.playback.PlaybackResolver
import dev.sorta.tv.playback.PlayerLauncher
import dev.sorta.tv.usb.UsbDriveLocator
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Substring search across `media.title` and `media.original_title`.
 * Triggered live as the user types (debounced ~250ms) and on submit.
 */
class SearchFragment :
    SearchSupportFragment(),
    SearchSupportFragment.SearchResultProvider {

    private lateinit var resultsAdapter: ArrayObjectAdapter
    private var driveRoot: File? = null
    private var pendingQuery: Job? = null
    private val playerLauncher: PlayerLauncher by lazy {
        PlayerLauncher(this, WatchHistory.get(requireContext()))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        resultsAdapter = ArrayObjectAdapter(ListRowPresenter())
        setSearchResultProvider(this)
        setOnItemViewClickedListener(OnItemViewClickedListener { _, item, _, _ ->
            if (item is MediaRow) onMediaClicked(item)
        })

        viewLifecycleOwnerLiveData.observe(this) { owner ->
            if (owner == null) return@observe
            owner.lifecycleScope.launch(Dispatchers.IO) {
                driveRoot = UsbDriveLocator.locate().firstOrNull()
            }
        }
    }

    override fun getResultsAdapter(): ObjectAdapter = resultsAdapter

    override fun onQueryTextChange(newQuery: String?): Boolean {
        runQuery(newQuery, debounce = true)
        return true
    }

    override fun onQueryTextSubmit(query: String?): Boolean {
        runQuery(query, debounce = false)
        return true
    }

    private fun runQuery(raw: String?, debounce: Boolean) {
        pendingQuery?.cancel()
        val query = raw?.trim().orEmpty()
        if (query.isEmpty()) {
            resultsAdapter.clear()
            return
        }
        pendingQuery = lifecycleScope.launch {
            if (debounce) delay(DEBOUNCE_MS)
            val drive = driveRoot ?: return@launch
            val rows = withContext(Dispatchers.IO) {
                MediaRepository.open(File(drive, "sorta.db")).use { it.search(query) }
            }
            renderResults(drive, query, rows)
        }
    }

    private fun renderResults(drive: File, query: String, rows: List<MediaRow>) {
        resultsAdapter.clear()
        if (rows.isEmpty()) return
        val header = HeaderItem(0, getString(R.string.search_results_header, query))
        val cards = ArrayObjectAdapter(CardPresenter(drive)).apply { addAll(0, rows) }
        resultsAdapter.add(ListRow(header, cards))
    }

    private fun onMediaClicked(media: MediaRow) {
        val drive = driveRoot ?: return
        when (media.mediaType) {
            MediaType.MOVIE -> launchMovie(drive, media)
            MediaType.TV -> startActivity(SeriesActivity.intentFor(requireContext(), drive, media))
        }
    }

    private fun launchMovie(drive: File, media: MediaRow) {
        val file = PlaybackResolver.resolve(drive, media)
        if (file == null) {
            Toast.makeText(
                requireContext(),
                getString(R.string.playback_no_file, media.title),
                Toast.LENGTH_LONG,
            ).show()
            return
        }
        playerLauncher.launch(file, WatchHistory.keyFor(drive, file))
    }

    private companion object {
        const val DEBOUNCE_MS = 250L
    }
}
