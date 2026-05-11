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
import dev.sorta.tv.data.Catalog
import dev.sorta.tv.data.CatalogAggregator
import dev.sorta.tv.data.CatalogCheck
import dev.sorta.tv.data.MediaRepository
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import dev.sorta.tv.data.WatchHistory
import dev.sorta.tv.playback.PlaybackResolver
import dev.sorta.tv.playback.PlayerLauncher
import dev.sorta.tv.playback.ResumeGate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Substring search across `media.title` and `media.original_title`,
 * fanned out across every plugged-in drive that passed
 * [CatalogCheck]'s max-version filter. Triggered live as the user
 * types (debounced ~250ms) and on submit.
 */
class SearchFragment :
    SearchSupportFragment(),
    SearchSupportFragment.SearchResultProvider {

    private lateinit var resultsAdapter: ArrayObjectAdapter
    private var driveRoots: List<File> = emptyList()
    private var pendingQuery: Job? = null
    private val playerLauncher = PlayerLauncher(this) {
        WatchHistory.get(requireContext())
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
                // Reuse the same multi-drive discovery the browse
                // screen uses so search hits exactly the drives the
                // user is browsing — and never an older-schema drive.
                val result = CatalogCheck.run()
                driveRoots = (result as? CatalogCheck.Result.Ok)?.driveRoots.orEmpty()
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
            val drives = driveRoots
            if (drives.isEmpty()) return@launch
            val rows = withContext(Dispatchers.IO) {
                openCatalog(drives).use { it.search(query) }
            }
            renderResults(query, rows)
        }
    }

    /** Open a single repo for one drive, or wrap N repos in an aggregator. */
    private fun openCatalog(drives: List<File>): Catalog {
        val backends = drives.map { File(it, "sorta.db") }.map(MediaRepository::open)
        return if (backends.size == 1) backends.single() else CatalogAggregator(backends)
    }

    private fun renderResults(query: String, rows: List<MediaRow>) {
        resultsAdapter.clear()
        if (rows.isEmpty()) return
        val allProgress = WatchHistory.get(requireContext()).progressUnder("")
        val header = HeaderItem(0, getString(R.string.search_results_header, query))
        val presenter = CardPresenter { media ->
            aggregateProgress(media.folderPath, allProgress)
        }
        val cards = ArrayObjectAdapter(presenter).apply { addAll(0, rows) }
        resultsAdapter.add(ListRow(header, cards))
    }

    /** Same shape as BrowseFragment.aggregateProgress — see there for rationale. */
    private fun aggregateProgress(
        folderPath: String,
        all: Map<String, WatchHistory.Progress>,
    ): WatchHistory.Progress? {
        val prefix = "$folderPath/"
        val entries = all.entries.filter { it.key.startsWith(prefix) }
        if (entries.isEmpty()) return null
        val anyWatched = entries.any { it.value.watched }
        val anyInProgress = entries.any { it.value.positionMs > 0 && !it.value.watched }
        return WatchHistory.Progress(
            positionMs = if (!anyWatched && anyInProgress) 1L else 0L,
            durationMs = 0L,
            watched = anyWatched,
        )
    }

    private fun onMediaClicked(media: MediaRow) {
        // The row carries the drive it came from; fall back to the
        // first known drive only as a sanity net for stale rows.
        val drive = media.driveRoot ?: driveRoots.firstOrNull() ?: return
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
        ResumeGate.launch(
            context = requireContext(),
            history = WatchHistory.get(requireContext()),
            launcher = playerLauncher,
            file = file,
            mediaKey = WatchHistory.keyFor(drive, file),
        )
    }

    private companion object {
        const val DEBOUNCE_MS = 250L
    }
}
