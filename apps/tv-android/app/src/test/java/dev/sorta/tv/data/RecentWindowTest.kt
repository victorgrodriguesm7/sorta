package dev.sorta.tv.data

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RecentWindowTest {

    @Test
    fun cutoffIsExactlyDaysAgoInIso8601Z() {
        // Pin the "now" so the test is deterministic. Tests run on the
        // JVM where `java.time` is always available, so we use it here
        // to build the epoch-millis input the production code expects.
        val now = Instant.parse("2026-05-11T12:34:56Z").toEpochMilli()
        val cutoff = RecentWindow.cutoff(days = 14, nowMillis = now)
        assertEquals("2026-04-27T12:34:56Z", cutoff)
    }

    @Test
    fun cutoffEndsWithZForUtc() {
        val cutoff = RecentWindow.cutoff(days = 7)
        assertTrue("expected trailing Z, got $cutoff", cutoff.endsWith("Z"))
        // ISO 8601 UTC at second precision is always 20 chars.
        assertEquals(20, cutoff.length)
    }

    @Test
    fun cutoffShorterWindowIsLater() {
        val now = Instant.parse("2026-05-11T12:34:56Z").toEpochMilli()
        val twoWeeks = RecentWindow.cutoff(days = 14, nowMillis = now)
        val oneWeek = RecentWindow.cutoff(days = 7, nowMillis = now)
        // String comparison works because ISO 8601 sorts lexically
        // == chronologically.
        assertTrue("$oneWeek must be later than $twoWeeks", oneWeek > twoWeeks)
    }
}
