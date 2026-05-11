package dev.sorta.tv.ui

import android.view.ViewGroup
import androidx.core.content.ContextCompat
import androidx.leanback.widget.ImageCardView
import androidx.leanback.widget.Presenter
import com.bumptech.glide.Glide
import dev.sorta.tv.R
import dev.sorta.tv.data.Episode
import dev.sorta.tv.data.WatchHistory
import java.io.File

/**
 * Renders one [Episode] as a Leanback [ImageCardView]. The catalog
 * doesn't ship per-episode artwork, so every episode card reuses the
 * series poster (or its TMDB CDN fallback). That's enough for users
 * to recognise which series they're inside while scanning a season's
 * episode list.
 *
 * Posters share the same dimensions as [CardPresenter] so a season
 * row visually matches a movie/series row on the browse screen.
 */
class EpisodePresenter(
    /** Drive root that [seriesPosterPath] is resolved against. */
    private val driveRoot: File,
    /** Series poster path relative to the drive root, if any. */
    private val seriesPosterPath: String?,
    /** TMDB CDN fallback for [seriesPosterPath]. */
    private val seriesPosterUrl: String?,
    /** Progress lookup for the watched/resume badge. */
    private val progressFor: (Episode) -> WatchHistory.Progress? = { null },
) : Presenter() {

    // Same poster aspect as CardPresenter so a season row lines up
    // with a movie/series row visually.
    private val cardWidthDp = 160
    private val cardHeightDp = 240

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
        }
        // Shared progress-overlay primitive — same shape as
        // CardPresenter. See ProgressOverlayDrawable kdoc.
        card.mainImageView.foreground = ProgressOverlayDrawable(context)
        return ViewHolder(card)
    }

    override fun onBindViewHolder(holder: ViewHolder, item: Any) {
        val episode = item as Episode
        val card = holder.view as ImageCardView
        card.titleText = episode.label
        card.contentText = episode.file.nameWithoutExtension.takeIf { it != episode.label }
        (card.mainImageView.foreground as? ProgressOverlayDrawable)?.state =
            ProgressOverlayState.from(progressFor(episode))

        val placeholder = ContextCompat.getDrawable(card.context, R.drawable.poster_placeholder)
        val localPoster = seriesPosterPath?.let { File(driveRoot, it) }
        when {
            localPoster != null && localPoster.exists() -> {
                Glide.with(card.context)
                    .load(localPoster)
                    .placeholder(placeholder)
                    .into(card.mainImageView)
            }
            seriesPosterUrl != null -> {
                Glide.with(card.context)
                    .load(seriesPosterUrl)
                    .placeholder(placeholder)
                    .into(card.mainImageView)
            }
            else -> {
                card.mainImage = placeholder
            }
        }
    }

    override fun onUnbindViewHolder(holder: ViewHolder) {
        val card = holder.view as ImageCardView
        card.titleText = null
        card.contentText = null
        card.mainImage = null
        (card.mainImageView.foreground as? ProgressOverlayDrawable)?.state =
            ProgressOverlayState.None
    }
}
