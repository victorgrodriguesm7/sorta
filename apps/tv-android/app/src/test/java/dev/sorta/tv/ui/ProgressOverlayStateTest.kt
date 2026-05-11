package dev.sorta.tv.ui

import dev.sorta.tv.data.WatchHistory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProgressOverlayStateTest {

    @Test
    fun nullProgressYieldsNone() {
        assertEquals(ProgressOverlayState.None, ProgressOverlayState.from(null))
    }

    @Test
    fun watchedYieldsWatchedStateRegardlessOfPosition() {
        // Even mid-position, the explicit watched flag wins. Drives
        // the scrim + "Watched" pill on cards.
        val watched = WatchHistory.Progress(positionMs = 1L, durationMs = 100L, watched = true)
        assertEquals(ProgressOverlayState.Watched, ProgressOverlayState.from(watched))
    }

    @Test
    fun zeroPositionYieldsNone() {
        val fresh = WatchHistory.Progress(positionMs = 0L, durationMs = 1000L, watched = false)
        assertEquals(ProgressOverlayState.None, ProgressOverlayState.from(fresh))
    }

    @Test
    fun inProgressYieldsFractionClampedTo01() {
        val half = WatchHistory.Progress(positionMs = 50L, durationMs = 100L, watched = false)
        val state = ProgressOverlayState.from(half)
        assertTrue("expected InProgress, got $state", state is ProgressOverlayState.InProgress)
        assertEquals(0.5f, (state as ProgressOverlayState.InProgress).fraction, 1e-6f)
    }

    @Test
    fun overOneHundredPercentClampsToOne() {
        val weird = WatchHistory.Progress(positionMs = 200L, durationMs = 100L, watched = false)
        val state = ProgressOverlayState.from(weird) as ProgressOverlayState.InProgress
        assertEquals(1f, state.fraction, 1e-6f)
    }

    @Test
    fun unknownDurationButNonzeroPositionUsesFallbackFraction() {
        // Some players don't report duration. We still want some
        // visible ring — pick an indeterminate fallback of ~10% so
        // the user gets a hint that *something* is in progress
        // without misleadingly showing a near-empty bar.
        val noDur = WatchHistory.Progress(positionMs = 30_000L, durationMs = 0L, watched = false)
        val state = ProgressOverlayState.from(noDur) as ProgressOverlayState.InProgress
        assertEquals(0.1f, state.fraction, 1e-6f)
    }
}
