package dev.sorta.tv.playback

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Pure formatter test — kept JVM-side so the resume-dialog label
 * shape doesn't drift silently. Anything involving the actual
 * `AlertDialog.Builder` lives in an instrumented test (none yet —
 * the dialog is verified by hand for Phase 1).
 */
class ResumeGateFormatTest {

    @Test
    fun underAnHourDropsHourComponent() {
        assertEquals("0:00", ResumeGate.formatPosition(0L))
        assertEquals("0:05", ResumeGate.formatPosition(5_000L))
        assertEquals("1:23", ResumeGate.formatPosition(83_000L))
        assertEquals("59:59", ResumeGate.formatPosition(59L * 60_000 + 59_000))
    }

    @Test
    fun atLeastAnHourShowsHourComponent() {
        assertEquals("1:00:00", ResumeGate.formatPosition(3_600_000L))
        assertEquals("1:23:45", ResumeGate.formatPosition(((1 * 3600 + 23 * 60 + 45) * 1000L)))
        assertEquals("10:00:00", ResumeGate.formatPosition(10L * 3_600_000L))
    }

    @Test
    fun negativeOrSubSecondClampsToZero() {
        assertEquals("0:00", ResumeGate.formatPosition(-1_000L))
        assertEquals("0:00", ResumeGate.formatPosition(500L))
    }
}
