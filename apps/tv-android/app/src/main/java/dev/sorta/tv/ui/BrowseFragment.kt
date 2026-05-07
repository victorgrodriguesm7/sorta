package dev.sorta.tv.ui

import android.os.Bundle
import android.content.Intent
import android.widget.Toast
import androidx.leanback.app.BrowseSupportFragment
import androidx.leanback.widget.ArrayObjectAdapter
import androidx.leanback.widget.HeaderItem
import androidx.leanback.widget.ListRow
import androidx.leanback.widget.ListRowPresenter
import androidx.leanback.widget.OnItemViewClickedListener
import androidx.lifecycle.lifecycleScope
import dev.sorta.tv.R
import dev.sorta.tv.data.GenreRow
import dev.sorta.tv.data.MediaRepository
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import dev.sorta.tv.data.WatchHistory
import dev.sorta.tv.playback.PlaybackResolver
import dev.sorta.tv.playback.PlayerLauncher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Renders the catalog as one Leanback row per movie genre plus one
 * "Series" row at the top. Genre row labels are the
 * `translated_name` from the desktop, falling back to the canonical
 * English name from TMDB.
 */
class BrowseFragment : BrowseSupportFragment() {

    private lateinit var rowsAdapter: ArrayObjectAdapter
    private var driveRoot: File? = null
    private val playerLauncher: PlayerLauncher by lazy {
        PlayerLauncher(this, WatchHistory.get(requireContext()))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = getString(R.string.app_name)
        headersState = HEADERS_ENABLED
        isHeadersTransitionOnBackEnabled = true
        rowsAdapter = ArrayObjectAdapter(ListRowPresenter())
        adapter = rowsAdapter

        onItemViewClickedListener = OnItemViewClickedListener { _, item, _, _ ->
            if (item is MediaRow) onMediaClicked(item)
        }
        setOnSearchClickedListener {
            startActivity(Intent(requireContext(), SearchActivity::class.java))
        }

        loadCatalog()
    }

    private fun onMediaClicked(media: MediaRow) {
        val root = driveRoot ?: return
        when (media.mediaType) {
            MediaType.MOVIE -> launchMovie(root, media)
            MediaType.TV -> startActivity(SeriesActivity.intentFor(requireContext(), root, media))
        }
    }

    private fun launchMovie(root: File, media: MediaRow) {
        val file = PlaybackResolver.resolve(root, media)
        if (file == null) {
            Toast.makeText(
                requireContext(),
                getString(R.string.playback_no_file, media.title),
                Toast.LENGTH_LONG,
            ).show()
            return
        }
        playerLauncher.launch(file, WatchHistory.keyFor(root, file))
    }

    private fun loadCatalog() {
        viewLifecycleOwnerLiveData.observe(this) { owner ->
            if (owner == null) return@observe
            owner.lifecycleScope.launch {
                val payload = withContext(Dispatchers.IO) { buildRows() }
                renderRows(payload)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        // Refresh progress badges after returning from the player.
        // Catalog data hasn't changed, so just rebuild row contents.
        loadCatalog()
    }

    private suspend fun buildRows(): CatalogPayload? {
        val drive = requireArguments().getString(ARG_DRIVE_ROOT)?.let(::File)
            ?: return null
        val progress = WatchHistory.get(requireContext()).progressUnder("")
        MediaRepository.open(File(drive, "sorta.db")).use { repo ->
            val series = repo.listSeries()
            val movieGenres = repo.listGenres(MediaType.MOVIE)
            val moviesByGenre = movieGenres.associateWith { genre ->
                repo.listMoviesByGenre(genre.id)
            }
            return CatalogPayload(drive, series, movieGenres, moviesByGenre, progress)
        }
    }

    private fun renderRows(payload: CatalogPayload?) {
        rowsAdapter.clear()
        if (payload == null) {
            driveRoot = null
            return
        }
        driveRoot = payload.driveRoot

        val cardPresenter = CardPresenter(payload.driveRoot) { media ->
            aggregateProgress(media.folderPath, payload.progress)
        }
        var headerId = 0L

        if (payload.series.isNotEmpty()) {
            val header = HeaderItem(headerId++, getString(R.string.row_series))
            val rowAdapter = ArrayObjectAdapter(cardPresenter).apply {
                addAll(0, payload.series)
            }
            rowsAdapter.add(ListRow(header, rowAdapter))
        }

        for (genre in payload.genres) {
            val movies = payload.moviesByGenre[genre].orEmpty()
            if (movies.isEmpty()) continue
            val header = HeaderItem(headerId++, genre.displayName)
            val rowAdapter = ArrayObjectAdapter(cardPresenter).apply {
                addAll(0, movies)
            }
            rowsAdapter.add(ListRow(header, rowAdapter))
        }
    }

    /**
     * Synthesise a [WatchHistory.Progress] for a [MediaRow] from every
     * watch-history entry under its folder. The returned value drives
     * the corner badge:
     *   - any episode watched → `watched = true`  (✓ check icon)
     *   - any episode partial → `positionMs > 0`  (▶ resume icon)
     * Position/duration values themselves are placeholders here —
     * the badge presenter only inspects which case fires, not the
     * exact ms.
     */
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

    private data class CatalogPayload(
        val driveRoot: File,
        val series: List<MediaRow>,
        val genres: List<GenreRow>,
        val moviesByGenre: Map<GenreRow, List<MediaRow>>,
        val progress: Map<String, WatchHistory.Progress>,
    )

    companion object {
        private const val ARG_DRIVE_ROOT = "drive_root"

        fun newInstance(driveRoot: File): BrowseFragment = BrowseFragment().apply {
            arguments = Bundle().apply { putString(ARG_DRIVE_ROOT, driveRoot.absolutePath) }
        }
    }
}
