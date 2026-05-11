package dev.sorta.tv.ui

import android.graphics.drawable.Drawable
import android.view.ViewGroup
import androidx.core.content.ContextCompat
import androidx.leanback.widget.ImageCardView
import androidx.leanback.widget.Presenter
import com.bumptech.glide.Glide
import dev.sorta.tv.R
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.WatchHistory
import java.io.File

/**
 * Renders one [MediaRow] as a Leanback [ImageCardView] — a poster
 * tile with a title underneath. Posters are loaded via Glide; the
 * D-pad focus highlight comes from the Leanback theme.
 *
 * Card size is fixed in dp so a row of TMDB w185 posters
 * (185×278 native) lays out without stretching on a 1080p TV.
 *
 * The watched / in-progress badge is sourced via [progressFor] so
 * the presenter has no compile-time dependency on the watch store —
 * the fragment hands in a snapshot map at render time.
 */
class CardPresenter(
    /** Drive root that [MediaRow.posterPath] is resolved against. */
    private val driveRoot: File,
    /** Progress lookup for badge rendering. Null means "no badge". */
    private val progressFor: (MediaRow) -> WatchHistory.Progress? = { null },
) : Presenter() {

    // 185 × 278 ≈ TMDB w185 native poster aspect ratio.
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
        // Foreground drawable on the main image renders the progress
        // ring + watched scrim/pill. Created here so it survives
        // recycler rebinds; the binder just updates `.state`.
        card.mainImageView.foreground = ProgressOverlayDrawable(context)
        return ViewHolder(card)
    }

    override fun onBindViewHolder(holder: ViewHolder, item: Any) {
        val media = item as MediaRow
        val card = holder.view as ImageCardView
        card.titleText = media.title
        card.contentText = media.originalTitle?.takeIf { it != media.title }

        // Progress / watched state is rendered as a foreground
        // overlay on the poster (see onCreateViewHolder). The legacy
        // corner badgeImage is left unset to avoid double-rendering.
        (card.mainImageView.foreground as? ProgressOverlayDrawable)?.state =
            ProgressOverlayState.from(progressFor(media))

        val placeholder: Drawable? = ContextCompat.getDrawable(
            card.context,
            R.drawable.poster_placeholder,
        )

        val localPoster = media.posterPath?.let { File(driveRoot, it) }
        when {
            localPoster != null && localPoster.exists() -> {
                Glide.with(card.context)
                    .load(localPoster)
                    .placeholder(placeholder)
                    .into(card.mainImageView)
            }
            media.posterUrl != null -> {
                Glide.with(card.context)
                    .load(media.posterUrl)
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
