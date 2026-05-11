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
import dev.sorta.tv.data.Catalog
import dev.sorta.tv.data.CatalogAggregator
import dev.sorta.tv.data.GenreRow
import dev.sorta.tv.data.MediaRepository
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import dev.sorta.tv.data.RecentWindow
import dev.sorta.tv.data.WatchHistory
import dev.sorta.tv.playback.PlaybackResolver
import dev.sorta.tv.playback.PlayerLauncher
import dev.sorta.tv.playback.ResumeGate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Renders the catalog as one Leanback row per movie genre plus one
 * "Series" row at the top. Genre row labels are the
 * `translated_name` from the desktop, falling back to the canonical
 * English name from TMDB.
 *
 * Backed by a [CatalogAggregator] when more than one drive is
 * plugged in — the rows are merged across drives transparently and
 * each [MediaRow] keeps a back-pointer to the HD it came from so
 * playback / poster resolution stays drive-correct.
 */
class BrowseFragment : BrowseSupportFragment() {

    private lateinit var rowsAdapter: ArrayObjectAdapter
    private var driveRoots: List<File> = emptyList()
    // Eager init (not `by lazy`) so registerForActivityResult fires
    // before the fragment reaches CREATED — see PlayerLauncher kdoc.
    private val playerLauncher = PlayerLauncher(this) {
        WatchHistory.get(requireContext())
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

    override fun onStart() {
        super.onStart()
        // TV remotes expose a dedicated MENU button; route it to
        // Settings so the language picker is reachable without
        // adding a custom title-view affordance. BrowseActivity also
        // routes the same keycode (see its onKeyDown) for cases
        // where the focused child swallowed the event first.
        view?.let { v ->
            v.isFocusableInTouchMode = true
            v.setOnKeyListener { _, keyCode, event ->
                if (event.action == android.view.KeyEvent.ACTION_DOWN &&
                    keyCode == android.view.KeyEvent.KEYCODE_MENU
                ) {
                    startActivity(Intent(requireContext(), SettingsActivity::class.java))
                    true
                } else {
                    false
                }
            }
        }
    }

    private fun onMediaClicked(media: MediaRow) {
        // Use the row's own driveRoot — in multi-drive mode the
        // catalog list is a merged view, so `driveRoots[0]` would
        // launch playback against the wrong HD for rows from later
        // drives. Fall back to the first known drive only as a
        // sanity net for legacy fixture rows (`driveRoot == null`).
        val root = media.driveRoot ?: driveRoots.firstOrNull() ?: return
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
        ResumeGate.launch(
            context = requireContext(),
            history = WatchHistory.get(requireContext()),
            launcher = playerLauncher,
            file = file,
            mediaKey = WatchHistory.keyFor(root, file),
        )
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
        val drives = requireArguments()
            .getStringArray(ARG_DRIVE_ROOTS)
            ?.map(::File)
            ?: return null
        if (drives.isEmpty()) return null
        val progress = WatchHistory.get(requireContext()).progressUnder("")
        val backends = drives.map { File(it, "sorta.db") }.map(MediaRepository::open)
        val catalog: Catalog = if (backends.size == 1) backends.single() else CatalogAggregator(backends)
        catalog.use { repo ->
            val series = repo.listSeries()
            val movieGenres = repo.listGenres(MediaType.MOVIE)
            val moviesByGenre = movieGenres.associateWith { genre ->
                repo.listMoviesByGenre(genre.id)
            }
            val recent = repo.listRecentlyAddedMovies(RecentWindow.cutoff())
            return CatalogPayload(drives, recent, series, movieGenres, moviesByGenre, progress)
        }
    }

    private fun renderRows(payload: CatalogPayload?) {
        rowsAdapter.clear()
        if (payload == null) {
            driveRoots = emptyList()
            return
        }
        driveRoots = payload.driveRoots

        val cardPresenter = CardPresenter { media ->
            aggregateProgress(media.mediaType, media.folderPath, payload.progress)
        }
        var headerId = 0L

        // "Recently added" sits above everything else — the whole
        // point of the row is to surface freshly catalogued items
        // ahead of the alphabetised genre rows below.
        if (payload.recentlyAdded.isNotEmpty()) {
            val header = HeaderItem(headerId++, getString(R.string.row_recently_added))
            val rowAdapter = ArrayObjectAdapter(cardPresenter).apply {
                addAll(0, payload.recentlyAdded)
            }
            rowsAdapter.add(ListRow(header, rowAdapter))
        }

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
     * watch-history entry under its folder. Drives the corner overlay:
     *   - movies: any partial position → in-progress ring; watched
     *     flag passes through verbatim.
     *   - series: only the in-progress ring may fire. "Watched" is
     *     intentionally suppressed at the series-card level because a
     *     show with one finished episode out of fifty is not really
     *     watched — the Watched pill should only appear on a specific
     *     episode inside [SeriesFragment].
     * Position/duration values themselves are placeholders — the
     * overlay only inspects which case fires, not the exact ms.
     */
    private fun aggregateProgress(
        mediaType: MediaType,
        folderPath: String,
        all: Map<String, WatchHistory.Progress>,
    ): WatchHistory.Progress? {
        val prefix = "$folderPath/"
        val entries = all.entries.filter { it.key.startsWith(prefix) }
        if (entries.isEmpty()) return null
        val anyWatched = entries.any { it.value.watched }
        val anyInProgress = entries.any { it.value.positionMs > 0 && !it.value.watched }
        val watchedFlag = when (mediaType) {
            MediaType.MOVIE -> anyWatched
            MediaType.TV -> false
        }
        return WatchHistory.Progress(
            positionMs = if (!watchedFlag && anyInProgress) 1L else 0L,
            durationMs = 0L,
            watched = watchedFlag,
        )
    }

    private data class CatalogPayload(
        val driveRoots: List<File>,
        /** Top-of-screen row: movies the user flagged + catalogued recently. */
        val recentlyAdded: List<MediaRow>,
        val series: List<MediaRow>,
        val genres: List<GenreRow>,
        val moviesByGenre: Map<GenreRow, List<MediaRow>>,
        val progress: Map<String, WatchHistory.Progress>,
    )

    companion object {
        private const val ARG_DRIVE_ROOTS = "drive_roots"

        fun newInstance(driveRoots: List<File>): BrowseFragment = BrowseFragment().apply {
            arguments = Bundle().apply {
                putStringArray(
                    ARG_DRIVE_ROOTS,
                    driveRoots.map(File::getAbsolutePath).toTypedArray(),
                )
            }
        }
    }
}
