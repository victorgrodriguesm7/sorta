package dev.sorta.tv.ui

import dev.sorta.tv.data.WatchHistory

/**
 * What the corner overlay should draw for a given playback state.
 * Lifted out of [ProgressOverlayDrawable] so the
 * progress-vs-watched-vs-none mapping stays unit-testable without
 * pulling Android graphics into a JVM test.
 */
sealed interface ProgressOverlayState {
    /** Don't draw anything — fresh, never-played media. */
    data object None : ProgressOverlayState

    /** Draw the dim scrim and the "Watched" pill. */
    data object Watched : ProgressOverlayState

    /**
     * Draw a partial progress ring. [fraction] is clamped to `[0, 1]`.
     * When duration is unknown but a position exists, we still
     * surface a small indeterminate fraction so the user sees
     * *something* — empty rings look identical to "fresh" media.
     */
    data class InProgress(val fraction: Float) : ProgressOverlayState

    companion object {
        /** Fallback ring fill when we know a position but not duration. */
        const val UNKNOWN_DURATION_FRACTION = 0.1f

        fun from(progress: WatchHistory.Progress?): ProgressOverlayState {
            if (progress == null) return None
            if (progress.watched) return Watched
            if (progress.positionMs <= 0L) return None
            val raw = if (progress.durationMs > 0L) {
                progress.positionMs.toFloat() / progress.durationMs
            } else {
                UNKNOWN_DURATION_FRACTION
            }
            return InProgress(raw.coerceIn(0f, 1f))
        }
    }
}
