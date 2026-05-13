package dev.sorta.tv.playback

import dev.sorta.tv.data.WatchHistory

/**
 * Pre-flight decision the caller takes before handing a video file
 * to the external player. Lifted out of [PlayerLauncher] so it's
 * unit-testable without a `Fragment` host.
 */
sealed interface ResumeDecision {
    /** No prior progress (or position 0) — start from the beginning. */
    data object Fresh : ResumeDecision

    /**
     * The media was already finished (`watched = true`) or is in the
     * last 5% of its duration. Start over from zero silently — no
     * dialog needed, the user clicked Play on something they'd
     * effectively already seen.
     */
    data object RestartFromZero : ResumeDecision

    /**
     * Partway through. Caller is expected to surface a "Continue
     * where you left off?" affordance before launching with
     * [positionMs] (or 0 if the user picks "Start over").
     */
    data class Resume(val positionMs: Long) : ResumeDecision
}

object ResumePolicy {

    /**
     * Cut-off above which we treat playback as "effectively done"
     * and silently restart on the next click. Matches the
     * auto-watched threshold in [WatchHistory.AUTO_WATCHED_FRACTION]
     * so the two heuristics agree.
     */
    private const val NEAR_END_FRACTION = 0.95

    fun decide(progress: WatchHistory.Progress?): ResumeDecision {
        if (progress == null) return ResumeDecision.Fresh
        if (progress.watched) return ResumeDecision.RestartFromZero

        val pos = progress.positionMs
        val dur = progress.durationMs
        if (pos <= 0L) return ResumeDecision.Fresh
        if (dur > 0L && pos.toDouble() / dur >= NEAR_END_FRACTION) {
            return ResumeDecision.RestartFromZero
        }
        return ResumeDecision.Resume(pos)
    }
}
