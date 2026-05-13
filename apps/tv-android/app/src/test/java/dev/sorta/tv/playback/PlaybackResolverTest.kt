package dev.sorta.tv.playback

import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class PlaybackResolverTest {

    @get:Rule val tmp = TemporaryFolder()

    @Test
    fun resolve_movie_picksTheVideoFileAtFolderRoot() {
        val root = tmp.root
        val movieFolder = File(root, "Movies/Action/Inception [tmdb-27205]").apply { mkdirs() }
        val expected = File(movieFolder, "Inception [tmdb-27205].mkv").apply { writeText("") }
        File(movieFolder, "Inception [tmdb-27205].en.srt").writeText("")

        val media = movieRow("Movies/Action/Inception [tmdb-27205]")

        assertEquals(expected, PlaybackResolver.resolve(root, media))
    }

    @Test
    fun resolve_movie_skipsHiddenInternalMarkers() {
        val root = tmp.root
        val movieFolder = File(root, "Movies/Action/X [tmdb-1]").apply { mkdirs() }
        // Pre-compression backup must be skipped per disk-format spec.
        File(movieFolder, "X [tmdb-1].original.mkv").writeText("")
        // In-flight encode must be skipped too.
        File(movieFolder, "X [tmdb-1].compressing.mkv").writeText("")
        val real = File(movieFolder, "X [tmdb-1].mkv").apply { writeText("") }

        val media = movieRow("Movies/Action/X [tmdb-1]")

        assertEquals(real, PlaybackResolver.resolve(root, media))
    }

    @Test
    fun resolve_series_returnsFirstEpisodeOfFirstSeason() {
        val root = tmp.root
        val seriesFolder = File(root, "Series/Show [tmdb-9]").apply { mkdirs() }
        val s1 = File(seriesFolder, "Season 1").apply { mkdirs() }
        val s2 = File(seriesFolder, "Season 2").apply { mkdirs() }
        val s1e1 = File(s1, "S01E01.mkv").apply { writeText("") }
        File(s1, "S01E02.mkv").writeText("")
        File(s2, "S02E01.mkv").writeText("")

        val media = MediaRow(
            id = 1, tmdbId = 9, mediaType = MediaType.TV,
            title = "Show", originalTitle = null, runtimeMinutes = null,
            posterPath = null, posterUrl = null,
            folderPath = "Series/Show [tmdb-9]",
        )

        assertEquals(s1e1, PlaybackResolver.resolve(root, media))
    }

    @Test
    fun resolve_returnsNullWhenFolderMissing() {
        val media = movieRow("Movies/Nope [tmdb-0]")
        assertNull(PlaybackResolver.resolve(tmp.root, media))
    }

    @Test
    fun resolve_returnsNullWhenNoVideoFiles() {
        val root = tmp.root
        val movieFolder = File(root, "Movies/X [tmdb-1]").apply { mkdirs() }
        File(movieFolder, "notes.txt").writeText("")

        val media = movieRow("Movies/X [tmdb-1]")
        assertNull(PlaybackResolver.resolve(root, media))
    }

    private fun movieRow(folderPath: String): MediaRow = MediaRow(
        id = 1, tmdbId = 1, mediaType = MediaType.MOVIE,
        title = "X", originalTitle = null, runtimeMinutes = null,
        posterPath = null, posterUrl = null,
        folderPath = folderPath,
    )
}
