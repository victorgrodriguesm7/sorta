package dev.sorta.tv.ui

import android.os.Bundle
import android.widget.Toast
import androidx.leanback.app.BrowseSupportFragment
import androidx.leanback.widget.ArrayObjectAdapter
import androidx.leanback.widget.HeaderItem
import androidx.leanback.widget.ListRow
import androidx.leanback.widget.ListRowPresenter
import androidx.leanback.widget.OnItemViewClickedListener
import androidx.lifecycle.lifecycleScope
import dev.sorta.tv.R
import dev.sorta.tv.data.Episode
import dev.sorta.tv.data.Season
import dev.sorta.tv.data.SeriesScanner
import dev.sorta.tv.data.WatchHistory
import dev.sorta.tv.playback.PlayerLauncher
import dev.sorta.tv.playback.ResumeGate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * One row per season, each row containing the season's episodes as
 * cards. Clicking an episode launches the same external-player
 * intent the browse screen uses for movies.
 */
class SeriesFragment : BrowseSupportFragment() {

    private lateinit var rowsAdapter: ArrayObjectAdapter
    private lateinit var driveRoot: File
    private lateinit var seriesRoot: File
    private var seriesPosterPath: String? = null
    private var seriesPosterUrl: String? = null
    private val playerLauncher = PlayerLauncher(this) {
        WatchHistory.get(requireContext())
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val args = requireActivity().intent
        driveRoot = File(args.getStringExtra(SeriesActivity.EXTRA_DRIVE_ROOT)!!)
        val folderPath = args.getStringExtra(SeriesActivity.EXTRA_FOLDER_PATH)!!
        seriesRoot = File(driveRoot, folderPath)
        seriesPosterPath = args.getStringExtra(SeriesActivity.EXTRA_POSTER_PATH)
        seriesPosterUrl = args.getStringExtra(SeriesActivity.EXTRA_POSTER_URL)
        title = args.getStringExtra(SeriesActivity.EXTRA_TITLE)
        headersState = HEADERS_ENABLED
        isHeadersTransitionOnBackEnabled = true

        rowsAdapter = ArrayObjectAdapter(ListRowPresenter())
        adapter = rowsAdapter

        onItemViewClickedListener = OnItemViewClickedListener { _, item, _, _ ->
            if (item is Episode) launchEpisode(item)
        }

        loadSeasons()
    }

    override fun onResume() {
        super.onResume()
        // Refresh per-episode progress badges after returning from
        // the player.
        loadSeasons()
    }

    private fun loadSeasons() {
        viewLifecycleOwnerLiveData.observe(this) { owner ->
            if (owner == null) return@observe
            owner.lifecycleScope.launch {
                val payload = withContext(Dispatchers.IO) {
                    val seasons = SeriesScanner.scan(seriesRoot)
                    val folderKey = WatchHistory.keyFor(driveRoot, seriesRoot)
                    val progress = WatchHistory.get(requireContext()).progressUnder(folderKey)
                    seasons to progress
                }
                renderSeasons(payload.first, payload.second)
            }
        }
    }

    private fun renderSeasons(
        seasons: List<Season>,
        progress: Map<String, WatchHistory.Progress>,
    ) {
        rowsAdapter.clear()
        if (seasons.isEmpty()) {
            Toast.makeText(
                requireContext(),
                getString(R.string.series_empty),
                Toast.LENGTH_LONG,
            ).show()
            return
        }
        val presenter = EpisodePresenter(
            driveRoot = driveRoot,
            seriesPosterPath = seriesPosterPath,
            seriesPosterUrl = seriesPosterUrl,
        ) { episode ->
            progress[WatchHistory.keyFor(driveRoot, episode.file)]
        }
        var headerId = 0L
        for (season in seasons) {
            val header = HeaderItem(headerId++, season.label)
            val row = ArrayObjectAdapter(presenter).apply { addAll(0, season.episodes) }
            rowsAdapter.add(ListRow(header, row))
        }
    }

    private fun launchEpisode(episode: Episode) {
        ResumeGate.launch(
            context = requireContext(),
            history = WatchHistory.get(requireContext()),
            launcher = playerLauncher,
            file = episode.file,
            mediaKey = WatchHistory.keyFor(driveRoot, episode.file),
        )
    }
}
