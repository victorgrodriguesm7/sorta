package dev.sorta.tv.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.recyclerview.widget.RecyclerView
import com.bumptech.glide.Glide
import dev.sorta.tv.R
import dev.sorta.tv.data.WatchHistory
import java.io.File

/**
 * Single-list adapter for the redesigned series screen. The list
 * is a flat sequence of [Row]s; the adapter picks one of two view
 * types per slot (header vs episode item) and binds accordingly.
 *
 * Click events bubble up via [onClick] — the fragment routes them
 * through [ResumeGate] like every other playback callsite.
 */
class EpisodeListAdapter(
    /** Drive root used to resolve relative still paths. */
    private val driveRoot: File,
    /** Series poster, used as a fallback when a row has no still. */
    private val seriesPosterPath: String?,
    private val seriesPosterUrl: String?,
    /**
     * Resolves a per-episode progress entry; null → no overlay. The
     * fragment owns the underlying map and mutates it before calling
     * [submit], so the closure stays valid for the adapter's
     * lifetime.
     */
    private val progressFor: (SeriesEpisodeItem) -> WatchHistory.Progress?,
    private val onClick: (SeriesEpisodeItem) -> Unit,
) : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    sealed interface Row {
        data class Header(val seasonNumber: Int) : Row
        data class Item(val item: SeriesEpisodeItem) : Row
    }

    private var rows: List<Row> = emptyList()

    fun submit(sections: List<SeriesEpisodeSection>) {
        rows = sections.flatMap { section ->
            buildList {
                add(Row.Header(section.seasonNumber))
                section.items.forEach { add(Row.Item(it)) }
            }
        }
        notifyDataSetChanged()
    }

    override fun getItemCount(): Int = rows.size

    override fun getItemViewType(position: Int): Int = when (rows[position]) {
        is Row.Header -> TYPE_HEADER
        is Row.Item -> TYPE_ITEM
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val inflater = LayoutInflater.from(parent.context)
        return when (viewType) {
            TYPE_HEADER -> HeaderHolder(
                inflater.inflate(R.layout.series_section_header, parent, false),
            )
            TYPE_ITEM -> ItemHolder(
                inflater.inflate(R.layout.series_episode_item, parent, false),
            )
            else -> error("unknown viewType: $viewType")
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        when (val row = rows[position]) {
            is Row.Header -> (holder as HeaderHolder).bind(row.seasonNumber)
            is Row.Item -> (holder as ItemHolder).bind(row.item)
        }
    }

    private inner class HeaderHolder(view: View) : RecyclerView.ViewHolder(view) {
        private val label: TextView = view.findViewById(R.id.section_label)
        fun bind(seasonNumber: Int) {
            val context = label.context
            label.text = context.getString(R.string.episode_list_season_header, seasonNumber)
        }
    }

    private inner class ItemHolder(view: View) : RecyclerView.ViewHolder(view) {
        private val still: ImageView = view.findViewById(R.id.episode_still)
        private val titleView: TextView = view.findViewById(R.id.episode_title)
        private val overviewView: TextView = view.findViewById(R.id.episode_overview)
        private val metaView: TextView = view.findViewById(R.id.episode_meta)
        private val overlay = ProgressOverlayDrawable(view.context)

        init {
            still.foreground = overlay
        }

        fun bind(item: SeriesEpisodeItem) {
            val context = itemView.context
            titleView.text = context.getString(
                R.string.episode_list_item_title,
                item.seasonNumber,
                item.episodeNumber,
                item.title,
            )
            val snippet = SeriesEpisodeMerger.snippet(item.overview, max = 200)
            if (snippet.isNullOrBlank()) {
                overviewView.visibility = View.GONE
            } else {
                overviewView.visibility = View.VISIBLE
                overviewView.text = snippet
            }

            val parts = buildList {
                item.airDate?.takeIf { it.isNotBlank() }?.let(::add)
                item.runtimeMinutes?.let {
                    add(context.getString(R.string.episode_list_runtime_minutes, it))
                }
            }
            if (parts.isEmpty()) {
                metaView.visibility = View.GONE
            } else {
                metaView.visibility = View.VISIBLE
                metaView.text = parts.joinToString(" · ")
            }

            // Still resolution order: local cached → TMDB still URL →
            // series-level poster fallback. The last branch makes a
            // pre-recatalog season still render *something* recognisable.
            val placeholder = ContextCompat.getDrawable(context, R.drawable.poster_placeholder)
            val localStill = item.stillPath?.let { File(driveRoot, it) }
            val localPoster = seriesPosterPath?.let { File(driveRoot, it) }
            when {
                localStill != null && localStill.exists() ->
                    Glide.with(context).load(localStill).placeholder(placeholder).into(still)
                item.stillUrl != null ->
                    Glide.with(context).load(item.stillUrl).placeholder(placeholder).into(still)
                localPoster != null && localPoster.exists() ->
                    Glide.with(context).load(localPoster).placeholder(placeholder).into(still)
                seriesPosterUrl != null ->
                    Glide.with(context).load(seriesPosterUrl).placeholder(placeholder).into(still)
                else -> still.setImageDrawable(placeholder)
            }

            overlay.state = ProgressOverlayState.from(progressFor(item))

            itemView.setOnClickListener { onClick(item) }
        }
    }

    companion object {
        private const val TYPE_HEADER = 0
        private const val TYPE_ITEM = 1
    }
}
