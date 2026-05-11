package dev.sorta.tv.ui

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.TextView
import android.widget.Toast
import androidx.core.content.ContextCompat
import androidx.fragment.app.Fragment
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.bumptech.glide.Glide
import dev.sorta.tv.R
import dev.sorta.tv.data.MediaRepository
import dev.sorta.tv.data.SeriesScanner
import dev.sorta.tv.data.WatchHistory
import dev.sorta.tv.playback.PlayerLauncher
import dev.sorta.tv.playback.ResumeGate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Vertical episode list for one series. Replaces the old
 * BrowseSupportFragment one-row-per-season UI. The list pulls
 * primarily from the `episodes` table (real TMDB metadata) and
 * augments / falls back to a filesystem walk via SeriesScanner so
 * v3 drives + half-recataloged drives still render every file.
 */
class SeriesFragment : Fragment() {

    private lateinit var driveRoot: File
    private lateinit var seriesRoot: File
    private var mediaId: Long = -1L
    private var seriesPosterPath: String? = null
    private var seriesPosterUrl: String? = null
    private var seriesTitle: String? = null

    private lateinit var episodeList: RecyclerView
    private lateinit var posterView: ImageView
    private lateinit var titleView: TextView
    private lateinit var adapter: EpisodeListAdapter

    /**
     * Latest watch-history snapshot for everything under [seriesRoot].
     * Mutated on each [loadEpisodes] pass; the adapter's `progressFor`
     * closure reads from this field so overlays redraw without us
     * having to swap the closure or rebuild the adapter.
     */
    private var progress: Map<String, WatchHistory.Progress> = emptyMap()

    private val playerLauncher = PlayerLauncher(this) {
        WatchHistory.get(requireContext())
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val args = requireActivity().intent
        driveRoot = File(args.getStringExtra(SeriesActivity.EXTRA_DRIVE_ROOT)!!)
        val folderPath = args.getStringExtra(SeriesActivity.EXTRA_FOLDER_PATH)!!
        seriesRoot = File(driveRoot, folderPath)
        mediaId = args.getLongExtra(SeriesActivity.EXTRA_MEDIA_ID, -1L)
        seriesPosterPath = args.getStringExtra(SeriesActivity.EXTRA_POSTER_PATH)
        seriesPosterUrl = args.getStringExtra(SeriesActivity.EXTRA_POSTER_URL)
        seriesTitle = args.getStringExtra(SeriesActivity.EXTRA_TITLE)
    }

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?,
    ): View = inflater.inflate(R.layout.fragment_series, container, false)

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        episodeList = view.findViewById(R.id.episode_list)
        posterView = view.findViewById(R.id.series_poster)
        titleView = view.findViewById(R.id.series_title)

        titleView.text = seriesTitle
        bindSeriesPoster()

        adapter = EpisodeListAdapter(
            driveRoot = driveRoot,
            seriesPosterPath = seriesPosterPath,
            seriesPosterUrl = seriesPosterUrl,
            // Closure reads the live `progress` field so each bind
            // reflects the latest WatchHistory snapshot — no need to
            // rebuild the adapter or swap the lookup on resume.
            progressFor = { item ->
                item.file?.let { f -> progress[WatchHistory.keyFor(driveRoot, f)] }
            },
            onClick = { launchEpisode(it) },
        )
        episodeList.layoutManager = LinearLayoutManager(requireContext())
        episodeList.adapter = adapter

        loadEpisodes()
    }

    override fun onResume() {
        super.onResume()
        // Refresh per-episode progress overlays after returning from
        // the player.
        loadEpisodes()
    }

    private fun bindSeriesPoster() {
        val placeholder = ContextCompat.getDrawable(requireContext(), R.drawable.poster_placeholder)
        val localPoster = seriesPosterPath?.let { File(driveRoot, it) }
        when {
            localPoster != null && localPoster.exists() ->
                Glide.with(this).load(localPoster).placeholder(placeholder).into(posterView)
            seriesPosterUrl != null ->
                Glide.with(this).load(seriesPosterUrl).placeholder(placeholder).into(posterView)
            else ->
                posterView.setImageDrawable(placeholder)
        }
    }

    private fun loadEpisodes() {
        lifecycleScope.launch {
            val (sections, prog) = withContext(Dispatchers.IO) {
                val tableRows = if (mediaId > 0) {
                    runCatching {
                        MediaRepository.open(File(driveRoot, "sorta.db")).use { it.listEpisodes(mediaId) }
                    }.getOrDefault(emptyList())
                } else emptyList()
                val diskSeasons = SeriesScanner.scan(seriesRoot)
                val merged = SeriesEpisodeMerger.merge(tableRows, diskSeasons, driveRoot)
                val folderKey = WatchHistory.keyFor(driveRoot, seriesRoot)
                val p = WatchHistory.get(requireContext()).progressUnder(folderKey)
                merged to p
            }

            if (sections.isEmpty() || sections.all { it.items.isEmpty() }) {
                Toast.makeText(
                    requireContext(),
                    getString(R.string.series_empty),
                    Toast.LENGTH_LONG,
                ).show()
            }

            // Update the progress snapshot then submit. The adapter's
            // bind pass reads the live `progress` field, so overlays
            // paint with the latest values without a closure swap and
            // RecyclerView keeps its scroll/focus across the player
            // round-trip.
            progress = prog
            adapter.submit(sections)
        }
    }

    private fun launchEpisode(item: SeriesEpisodeItem) {
        val file = item.file ?: run {
            Toast.makeText(
                requireContext(),
                getString(R.string.playback_no_file, item.title),
                Toast.LENGTH_LONG,
            ).show()
            return
        }
        ResumeGate.launch(
            context = requireContext(),
            history = WatchHistory.get(requireContext()),
            launcher = playerLauncher,
            file = file,
            mediaKey = WatchHistory.keyFor(driveRoot, file),
        )
    }
}
