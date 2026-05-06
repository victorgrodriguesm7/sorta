package dev.sorta.tv.ui

import android.os.Bundle
import android.content.Intent
import android.net.Uri
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
import dev.sorta.tv.playback.PlaybackIntent
import dev.sorta.tv.playback.PlaybackResolver
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = getString(R.string.app_name)
        headersState = HEADERS_ENABLED
        isHeadersTransitionOnBackEnabled = true
        rowsAdapter = ArrayObjectAdapter(ListRowPresenter())
        adapter = rowsAdapter

        onItemViewClickedListener = OnItemViewClickedListener { _, item, _, _ ->
            if (item is MediaRow) launchPlayback(item)
        }
        setOnSearchClickedListener {
            startActivity(Intent(requireContext(), SearchActivity::class.java))
        }

        loadCatalog()
    }

    private fun launchPlayback(media: MediaRow) {
        val root = driveRoot ?: return
        val file = PlaybackResolver.resolve(root, media)
        if (file == null) {
            Toast.makeText(
                requireContext(),
                getString(R.string.playback_no_file, media.title),
                Toast.LENGTH_LONG,
            ).show()
            return
        }
        val request = PlaybackIntent.build(file)
        val play = Intent(request.action)
            .setDataAndType(Uri.parse(request.uri), request.mimeType)
            .addFlags(request.flags)
        // Wrap in createChooser so the user can pick VLC / MX Player /
        // whatever they have installed instead of being silently
        // routed to whichever player claims highest priority.
        startActivity(Intent.createChooser(play, getString(R.string.playback_chooser_title)))
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

    private suspend fun buildRows(): CatalogPayload? {
        val drive = requireArguments().getString(ARG_DRIVE_ROOT)?.let(::File)
            ?: return null
        MediaRepository.open(File(drive, "sorta.db")).use { repo ->
            val series = repo.listSeries()
            val movieGenres = repo.listGenres(MediaType.MOVIE)
            val moviesByGenre = movieGenres.associateWith { genre ->
                repo.listMoviesByGenre(genre.id)
            }
            return CatalogPayload(drive, series, movieGenres, moviesByGenre)
        }
    }

    private fun renderRows(payload: CatalogPayload?) {
        rowsAdapter.clear()
        if (payload == null) {
            driveRoot = null
            return
        }
        driveRoot = payload.driveRoot

        val cardPresenter = CardPresenter(payload.driveRoot)
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

    private data class CatalogPayload(
        val driveRoot: File,
        val series: List<MediaRow>,
        val genres: List<GenreRow>,
        val moviesByGenre: Map<GenreRow, List<MediaRow>>,
    )

    companion object {
        private const val ARG_DRIVE_ROOT = "drive_root"

        fun newInstance(driveRoot: File): BrowseFragment = BrowseFragment().apply {
            arguments = Bundle().apply { putString(ARG_DRIVE_ROOT, driveRoot.absolutePath) }
        }
    }
}
