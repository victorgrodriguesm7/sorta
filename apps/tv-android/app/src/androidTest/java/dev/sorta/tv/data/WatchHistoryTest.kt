package dev.sorta.tv.data

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Instrumented because [WatchHistory] uses Android's
 * [android.database.sqlite.SQLiteOpenHelper]. Each test runs against
 * a fresh DB by deleting the file before opening.
 */
@RunWith(AndroidJUnit4::class)
class WatchHistoryTest {

    private lateinit var history: WatchHistory

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        ctx.deleteDatabase("watch_history.db")
        history = WatchHistory.get(ctx)
    }

    @After
    fun tearDown() {
        history.close()
        InstrumentationRegistry.getInstrumentation()
            .targetContext.deleteDatabase("watch_history.db")
    }

    @Test
    fun progress_isNullForUnknownKey() {
        assertNull(history.progressFor("Movies/X/X.mkv"))
    }

    @Test
    fun record_storesPositionAndDuration() {
        history.record("Movies/X/X.mkv", positionMs = 60_000, durationMs = 600_000)
        val p = history.progressFor("Movies/X/X.mkv")
        assertNotNull(p)
        assertEquals(60_000L, p!!.positionMs)
        assertEquals(600_000L, p.durationMs)
        assertFalse(p.watched)
        assertEquals(0.1f, p.fraction, 0.001f)
    }

    @Test
    fun record_autoWatchesWhenPastThreshold() {
        // 95% of 100s → boundary case; auto-mark watched.
        history.record("Movies/X/X.mkv", positionMs = 95_000, durationMs = 100_000)
        assertTrue(history.progressFor("Movies/X/X.mkv")!!.watched)
    }

    @Test
    fun record_doesNotAutoWatchBelowThreshold() {
        history.record("Movies/X/X.mkv", positionMs = 50_000, durationMs = 100_000)
        assertFalse(history.progressFor("Movies/X/X.mkv")!!.watched)
    }

    @Test
    fun setWatched_flipsFlagWithoutWipingPosition() {
        history.record("Movies/X/X.mkv", positionMs = 30_000, durationMs = 100_000)
        history.setWatched("Movies/X/X.mkv", true)
        val p = history.progressFor("Movies/X/X.mkv")!!
        assertTrue(p.watched)
        assertEquals(30_000L, p.positionMs)
    }

    @Test
    fun setWatched_createsRowForNewKey() {
        history.setWatched("Series/Y/Season 1/S01E01.mkv", true)
        val p = history.progressFor("Series/Y/Season 1/S01E01.mkv")!!
        assertTrue(p.watched)
        assertEquals(0L, p.positionMs)
        assertEquals(0L, p.durationMs)
    }

    @Test
    fun progressUnder_returnsOnlyMatchingPrefix() {
        history.record("Series/Show [tmdb-1]/Season 1/S01E01.mkv", 0, 0)
        history.record("Series/Show [tmdb-1]/Season 1/S01E02.mkv", 0, 0)
        history.record("Series/Other/S01E01.mkv", 0, 0)

        val under = history.progressUnder("Series/Show [tmdb-1]")

        assertEquals(2, under.size)
        assertTrue(under.containsKey("Series/Show [tmdb-1]/Season 1/S01E01.mkv"))
        assertTrue(under.containsKey("Series/Show [tmdb-1]/Season 1/S01E02.mkv"))
    }

    @Test
    fun keyFor_stripsDriveRootAndNormalizesSlashes() {
        val drive = File("/storage/8C3C8F9A3C8F7DC8")
        val file = File("/storage/8C3C8F9A3C8F7DC8/Movies/Ação/X [tmdb-1]/X [tmdb-1].mkv")
        assertEquals(
            "Movies/Ação/X [tmdb-1]/X [tmdb-1].mkv",
            WatchHistory.keyFor(drive, file),
        )
    }
}
