package dev.sorta.tv.playback

import dev.sorta.tv.data.WatchHistory
import org.junit.Assert.assertEquals
import org.junit.Test

class ResumePolicyTest {

    @Test
    fun noProgressYieldsFresh() {
        assertEquals(ResumeDecision.Fresh, ResumePolicy.decide(null))
    }

    @Test
    fun zeroPositionYieldsFresh() {
        val p = WatchHistory.Progress(positionMs = 0L, durationMs = 5_000L, watched = false)
        assertEquals(ResumeDecision.Fresh, ResumePolicy.decide(p))
    }

    @Test
    fun watchedYieldsRestartFromZero() {
        // Even with a non-zero position, an explicit watched=true must
        // win — the user already finished this and is hitting Play
        // again, so start over.
        val p = WatchHistory.Progress(positionMs = 1_000L, durationMs = 10_000L, watched = true)
        assertEquals(ResumeDecision.RestartFromZero, ResumePolicy.decide(p))
    }

    @Test
    fun atLeast95PercentYieldsRestartFromZero() {
        // 95% on the nose: credits territory, restart.
        val p = WatchHistory.Progress(positionMs = 9_500L, durationMs = 10_000L, watched = false)
        assertEquals(ResumeDecision.RestartFromZero, ResumePolicy.decide(p))
    }

    @Test
    fun above95PercentYieldsRestartFromZero() {
        val p = WatchHistory.Progress(positionMs = 9_999L, durationMs = 10_000L, watched = false)
        assertEquals(ResumeDecision.RestartFromZero, ResumePolicy.decide(p))
    }

    @Test
    fun midwayYieldsResume() {
        val p = WatchHistory.Progress(positionMs = 5_000L, durationMs = 10_000L, watched = false)
        assertEquals(ResumeDecision.Resume(5_000L), ResumePolicy.decide(p))
    }

    @Test
    fun positionWithoutDurationYieldsResume() {
        // Some players don't report duration on first save. Treat a
        // recorded position as "in progress" rather than dropping back
        // to Fresh — losing the resume point would be worse than the
        // edge case of restarting a near-end watch.
        val p = WatchHistory.Progress(positionMs = 30_000L, durationMs = 0L, watched = false)
        assertEquals(ResumeDecision.Resume(30_000L), ResumePolicy.decide(p))
    }

    @Test
    fun resumePositionMsExposed() {
        val decision = ResumePolicy.decide(
            WatchHistory.Progress(positionMs = 12_345L, durationMs = 100_000L, watched = false),
        ) as ResumeDecision.Resume
        assertEquals(12_345L, decision.positionMs)
    }
}
