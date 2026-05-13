package dev.sorta.tv.ui

import dev.sorta.tv.data.Episode
import dev.sorta.tv.data.EpisodeRow
import dev.sorta.tv.data.Season
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SeriesEpisodeMergerTest {

    @Test
    fun preferTableRowsWhenPresent() {
        // Episodes table is authoritative because it carries real
        // TMDB metadata (title, overview, still). Disk-derived rows
        // for the same (season, episode) are dropped.
        val table = listOf(
            episodeRow(1, 1, title = "Pilot", overview = "First."),
            episodeRow(1, 2, title = "Second", overview = "Second."),
        )
        val disk = listOf(
            Season(
                label = "Season 1",
                number = 1,
                episodes = listOf(
                    Episode(File("/x/S01E01.mkv"), "S01E01", 1, 1),
                    Episode(File("/x/S01E02.mkv"), "S01E02", 1, 2),
                ),
            ),
        )
        val merged = SeriesEpisodeMerger.merge(table, disk)
        assertEquals(1, merged.size)
        val season = merged.single()
        assertEquals(1, season.seasonNumber)
        assertEquals(2, season.items.size)
        // Table title beats the SxxExx-derived label.
        assertEquals("Pilot", season.items[0].title)
        assertEquals("First.", season.items[0].overview)
        // File path comes from the table row.
        // Normalize separators — on Windows test hosts `File("/x/…").path`
        // comes back with backslashes; the production target (Android)
        // keeps forward slashes.
        assertEquals(
            "/x/s01e01.mkv",
            season.items[0].file?.path?.replace('\\', '/')?.lowercase(),
        )
    }

    @Test
    fun synthesizeFromDiskWhenTableEmpty() {
        // v3 drives / pre-recatalog rows have no episodes table data.
        // We fall back to SeriesScanner output so the screen isn't
        // empty.
        val disk = listOf(
            Season(
                label = "Season 1",
                number = 1,
                episodes = listOf(
                    Episode(File("/x/S01E01.mkv"), "S01E01", 1, 1),
                ),
            ),
        )
        val merged = SeriesEpisodeMerger.merge(table = emptyList(), disk = disk)
        assertEquals(1, merged.single().items.size)
        val item = merged.single().items.single()
        // No TMDB metadata — title falls back to the SxxExx label and
        // overview is null (UI renders an em-dash).
        assertEquals("S01E01", item.title)
        assertNull(item.overview)
        assertNull(item.stillPath)
    }

    @Test
    fun appendDiskOnlyEntriesNotInTable() {
        // Half-recataloged drive: table has S01E01 but disk has both
        // S01E01 and S01E02. The merger keeps the table row for
        // S01E01 and appends a synthetic row for S01E02.
        val table = listOf(episodeRow(1, 1, title = "Pilot"))
        val disk = listOf(
            Season(
                label = "Season 1",
                number = 1,
                episodes = listOf(
                    Episode(File("/x/S01E01.mkv"), "S01E01", 1, 1),
                    Episode(File("/x/S01E02.mkv"), "S01E02", 1, 2),
                ),
            ),
        )
        val merged = SeriesEpisodeMerger.merge(table, disk)
        val season = merged.single()
        assertEquals(2, season.items.size)
        assertEquals("Pilot", season.items[0].title)
        assertEquals("S01E02", season.items[1].title)
        assertNull(season.items[1].overview)
    }

    @Test
    fun groupAndSortBySeasonThenEpisode() {
        // Inputs are deliberately out of order — the merger has to
        // produce a stable display order.
        val table = listOf(
            episodeRow(2, 1, title = "S2E1"),
            episodeRow(1, 2, title = "S1E2"),
            episodeRow(1, 1, title = "S1E1"),
        )
        val merged = SeriesEpisodeMerger.merge(table, disk = emptyList())
        assertEquals(listOf(1, 2), merged.map { it.seasonNumber })
        assertEquals(listOf("S1E1", "S1E2"), merged[0].items.map { it.title })
        assertEquals(listOf("S2E1"), merged[1].items.map { it.title })
    }

    @Test
    fun overviewTruncatedToTwoHundredChars() {
        // The plan calls for "first 200 characters" of the synopsis
        // on the episode list item. The merger leaves the full text
        // on the model; truncation is a renderer concern — keep this
        // assertion at "the full text survives" so we don't bake the
        // truncation policy into the data layer.
        val full = "x".repeat(500)
        val merged = SeriesEpisodeMerger.merge(
            table = listOf(episodeRow(1, 1, overview = full)),
            disk = emptyList(),
        )
        assertEquals(500, merged.single().items.single().overview!!.length)
        // And the helper for the renderer trims at 200 + ellipsis.
        assertEquals(
            "x".repeat(200) + "…",
            SeriesEpisodeMerger.snippet(full, max = 200),
        )
        assertTrue(SeriesEpisodeMerger.snippet("short", max = 200) == "short")
        assertNull(SeriesEpisodeMerger.snippet(null, max = 200))
    }

    private fun episodeRow(
        season: Int,
        episode: Int,
        title: String? = null,
        overview: String? = null,
    ) = EpisodeRow(
        id = (season * 100 + episode).toLong(),
        mediaId = 1L,
        seasonNumber = season,
        episodeNumber = episode,
        title = title,
        overview = overview,
        airDate = null,
        runtimeMinutes = null,
        stillPath = null,
        stillUrl = null,
        filePath = "/x/S${"%02d".format(season)}E${"%02d".format(episode)}.mkv",
    )
}
