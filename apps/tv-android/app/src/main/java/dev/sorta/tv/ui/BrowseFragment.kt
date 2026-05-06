package dev.sorta.tv.ui

import android.os.Bundle
import androidx.leanback.app.BrowseSupportFragment
import androidx.leanback.widget.ArrayObjectAdapter
import androidx.leanback.widget.HeaderItem
import androidx.leanback.widget.ListRow
import androidx.leanback.widget.ListRowPresenter
import androidx.lifecycle.lifecycleScope
import dev.sorta.tv.R
import dev.sorta.tv.data.GenreRow
import dev.sorta.tv.data.MediaRepository
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import dev.sorta.tv.usb.UsbDriveLocator
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = getString(R.string.app_name)
        headersState = HEADERS_ENABLED
        isHeadersTransitionOnBackEnabled = true
        rowsAdapter = ArrayObjectAdapter(ListRowPresenter())
        adapter = rowsAdapter

        loadCatalog()
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
        val driveRoot = UsbDriveLocator.locate().firstOrNull() ?: return null
        MediaRepository.open(File(driveRoot, "sorta.db")).use { repo ->
            val series = repo.listSeries()
            val movieGenres = repo.listGenres(MediaType.MOVIE)
            val moviesByGenre = movieGenres.associateWith { genre ->
                repo.listMoviesByGenre(genre.id)
            }
            return CatalogPayload(driveRoot, series, movieGenres, moviesByGenre)
        }
    }

    private fun renderRows(payload: CatalogPayload?) {
        rowsAdapter.clear()
        if (payload == null) return

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
}
