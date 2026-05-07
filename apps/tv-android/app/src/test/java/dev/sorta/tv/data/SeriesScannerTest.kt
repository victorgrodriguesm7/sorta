package dev.sorta.tv.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class SeriesScannerTest {

    @get:Rule val tmp = TemporaryFolder()

    @Test
    fun scan_buildsSeasonsSortedByNumber() {
        val root = tmp.newFolder("Show [tmdb-1]")
        // Deliberately create out of order so the sort is exercised.
        seasonWithEpisodes(root, "Season 2", listOf("S02E01.mkv", "S02E02.mkv"))
        seasonWithEpisodes(root, "Season 1", listOf("S01E01.mkv", "S01E02.mkv"))
        seasonWithEpisodes(root, "Season 10", listOf("S10E01.mkv"))

        val seasons = SeriesScanner.scan(root)

        assertEquals(listOf(1, 2, 10), seasons.map { it.number })
    }

    @Test
    fun scan_episodesAreSortedByParsedNumber() {
        val root = tmp.newFolder("Show [tmdb-1]")
        seasonWithEpisodes(root, "Season 1", listOf("S01E10.mkv", "S01E02.mkv", "S01E01.mkv"))

        val episodes = SeriesScanner.scan(root).single().episodes

        assertEquals(listOf(1, 2, 10), episodes.map { it.episodeNumber })
        assertEquals(listOf("S01E01", "S01E02", "S01E10"), episodes.map { it.label })
    }

    @Test
    fun scan_acceptsTranslatedSeasonLabel() {
        // "Temporada" is the desktop's pt-BR default for season_label.
        val root = tmp.newFolder("Show [tmdb-1]")
        seasonWithEpisodes(root, "Temporada 1", listOf("S01E01.mkv"))

        val seasons = SeriesScanner.scan(root)

        assertEquals(1, seasons.size)
        assertEquals("Temporada 1", seasons.single().label)
        assertEquals(1, seasons.single().number)
    }

    @Test
    fun scan_keepsBareBasenameWhenEpisodeTagAbsent() {
        val root = tmp.newFolder("Show [tmdb-1]")
        seasonWithEpisodes(root, "Season 1", listOf("Pilot.mkv", "Heart of Gold.mkv"))

        val episodes = SeriesScanner.scan(root).single().episodes

        // Both unparsed → sort alphabetically by the basename.
        assertEquals(listOf("Heart of Gold", "Pilot"), episodes.map { it.label })
        assertTrue(episodes.all { it.episodeNumber == null })
    }

    @Test
    fun scan_dropsSeasonsWithNoPlayableFiles() {
        val root = tmp.newFolder("Show [tmdb-1]")
        seasonWithEpisodes(root, "Season 1", listOf("S01E01.mkv"))
        // Empty + non-video-only → both should be hidden from the UI.
        File(root, "Specials").apply { mkdirs() }
        seasonWithEpisodes(root, "Notes", listOf("readme.txt"))

        val seasons = SeriesScanner.scan(root)

        assertEquals(listOf("Season 1"), seasons.map { it.label })
    }

    @Test
    fun scan_skipsHiddenSortaInternalFiles() {
        val root = tmp.newFolder("Show [tmdb-1]")
        seasonWithEpisodes(
            root,
            "Season 1",
            listOf(
                "S01E01.mkv",
                "S01E01.original.mkv",     // pre-compression backup; hide
                "S01E02.compressing.mkv",  // in-flight encode; hide
            ),
        )

        val episodes = SeriesScanner.scan(root).single().episodes

        assertEquals(1, episodes.size)
        assertEquals("S01E01", episodes.single().label)
    }

    @Test
    fun scan_returnsEmptyForMissingDirectory() {
        val ghost = File(tmp.root, "does-not-exist")
        assertTrue(SeriesScanner.scan(ghost).isEmpty())
    }

    @Test
    fun scan_seasonNumberFallsBackToNullWhenLabelHasNoDigits() {
        val root = tmp.newFolder("Show [tmdb-1]")
        seasonWithEpisodes(root, "Specials", listOf("S00E01.mkv"))

        val season = SeriesScanner.scan(root).single()
        assertNull(season.number)
        assertEquals("Specials", season.label)
    }

    private fun seasonWithEpisodes(parent: File, name: String, episodes: List<String>): File {
        val dir = File(parent, name).apply { mkdirs() }
        for (e in episodes) File(dir, e).writeText("")
        return dir
    }
}
