package dev.sorta.tv.ui

import android.view.ViewGroup
import androidx.core.content.ContextCompat
import androidx.leanback.widget.ImageCardView
import androidx.leanback.widget.Presenter
import dev.sorta.tv.R
import dev.sorta.tv.data.Episode
import dev.sorta.tv.data.WatchHistory

/**
 * Renders one [Episode] as a Leanback [ImageCardView]. We don't have
 * per-episode thumbnails in the catalog (TMDB still images would be
 * a future feature), so the main image is the same film-strip glyph
 * used by the poster placeholder, and the episode label fills the
 * card title.
 *
 * Card is wider than the poster card and shorter, since episode
 * labels read better in landscape and there's no portrait artwork.
 */
class EpisodePresenter(
    /** Progress lookup for the watched/resume badge. */
    private val progressFor: (Episode) -> WatchHistory.Progress? = { null },
) : Presenter() {

    private val cardWidthDp = 220
    private val cardHeightDp = 124

    override fun onCreateViewHolder(parent: ViewGroup): ViewHolder {
        val context = parent.context
        val card = ImageCardView(context).apply {
            isFocusable = true
            isFocusableInTouchMode = true
            val density = resources.displayMetrics.density
            setMainImageDimensions(
                (cardWidthDp * density).toInt(),
                (cardHeightDp * density).toInt(),
            )
            setBackgroundColor(
                ContextCompat.getColor(context, R.color.card_background),
            )
            mainImage = ContextCompat.getDrawable(context, R.drawable.poster_placeholder)
        }
        return ViewHolder(card)
    }

    override fun onBindViewHolder(holder: ViewHolder, item: Any) {
        val episode = item as Episode
        val card = holder.view as ImageCardView
        card.titleText = episode.label
        card.contentText = episode.file.nameWithoutExtension.takeIf { it != episode.label }
        card.badgeImage = badgeFor(card.context, progressFor(episode))
    }

    override fun onUnbindViewHolder(holder: ViewHolder) {
        val card = holder.view as ImageCardView
        card.titleText = null
        card.contentText = null
        card.badgeImage = null
    }
}
