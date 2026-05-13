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
 * Instrumented test against the fixture sorta.db committed under
 * `app/src/androidTest/assets/sorta.db`. Copies the asset to the
 * instrumentation context's cache dir on each run so tests can open
 * a real file path with `OPEN_READONLY`.
 */
@RunWith(AndroidJUnit4::class)
class MediaRepositoryTest {

    private lateinit var dbFile: File
    private lateinit var repo: MediaRepository

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().context
        dbFile = File(ctx.cacheDir, "fixture-sorta.db")
        ctx.assets.open("sorta.db").use { input ->
            dbFile.outputStream().use { output -> input.copyTo(output) }
        }
        repo = MediaRepository.open(dbFile)
    }

    @After
    fun tearDown() {
        repo.close()
        dbFile.delete()
    }

    @Test
    fun listGenres_returnsMovieAndTvGenresSortedByDisplayName() {
        val all = repo.listGenres()
        // Fixture: 3 movie genres (Ação, Crime, Drama) + 1 tv genre (Drama).
        assertEquals(4, all.size)
        // Display name uses the translated_name when set.
        assertEquals(listOf("Ação", "Crime", "Drama", "Drama"), all.map { it.displayName })
    }

    @Test
    fun listGenres_filtersByMediaType() {
        val movieGenres = repo.listGenres(MediaType.MOVIE)
        assertEquals(3, movieGenres.size)
        assertTrue(movieGenres.all { it.mediaType == MediaType.MOVIE })

        val tvGenres = repo.listGenres(MediaType.TV)
        assertEquals(1, tvGenres.size)
        assertEquals("Drama", tvGenres.single().canonicalName)
    }

    @Test
    fun listMoviesByGenre_defaultsToPrimaryOnly() {
        // Primary genre matches.
        val action = repo.listMoviesByGenre(28L)
        assertEquals(1, action.size)
        assertEquals("A Origem", action.single().title)

        val crime = repo.listMoviesByGenre(80L)
        assertEquals(1, crime.size)
        assertEquals("Cidade de Deus", crime.single().title)

        // Drama is only a *secondary* genre on Cidade de Deus, so by
        // default (primaryOnly = true) it returns no rows — that's
        // what makes each movie appear exactly once in the browse UI.
        assertTrue(repo.listMoviesByGenre(18L).isEmpty())
    }

    @Test
    fun listMoviesByGenre_canOptInToSecondaryMatches() {
        val drama = repo.listMoviesByGenre(18L, primaryOnly = false)
        assertEquals(1, drama.size)
        assertEquals("Cidade de Deus", drama.single().title)
    }

    @Test
    fun listMoviesByGenre_returnsEmptyForUnknownGenre() {
        assertTrue(repo.listMoviesByGenre(99999L).isEmpty())
    }

    @Test
    fun listSeries_returnsTvOnly() {
        val series = repo.listSeries()
        assertEquals(1, series.size)
        val gameOfThrones = series.single()
        assertEquals(MediaType.TV, gameOfThrones.mediaType)
        assertEquals(1399L, gameOfThrones.tmdbId)
        assertEquals("Series/Game of Thrones [tmdb-1399]", gameOfThrones.folderPath)
    }

    @Test
    fun setting_readsKnownAndMissingKeys() {
        assertEquals("4", repo.setting("schema_version"))
        assertEquals("Movies", repo.setting("movies_folder_label"))
        assertEquals("Season", repo.setting("season_label"))
        assertNull(repo.setting("nonexistent_key"))
    }

    @Test
    fun mediaRow_carriesIsNewAndCataloguedAt() {
        // Inception was inserted with is_new = 1 and a "now" timestamp;
        // Cidade de Deus with is_new = 0 and a 2024-01-15 timestamp.
        val inception = repo.listMoviesByGenre(28L).single { it.title == "A Origem" }
        assertTrue(inception.isNew)
        assertNotNull(inception.cataloguedAt)
        assertTrue(
            "expected ISO 8601 UTC, got ${inception.cataloguedAt}",
            inception.cataloguedAt!!.endsWith("Z"),
        )

        val crime = repo.listMoviesByGenre(80L).single()
        assertFalse(crime.isNew)
        assertEquals("2024-01-15T10:00:00Z", crime.cataloguedAt)
    }

    @Test
    fun listEpisodes_returnsEmptyForSeriesWithoutEpisodes() {
        // Fake media id that exists but has no episodes rows.
        assertTrue(repo.listEpisodes(mediaId = 1L).isEmpty())
    }

    @Test
    fun listEpisodes_returnsAllEpisodesSortedBySeasonThenEpisode() {
        // Game of Thrones has S01E01, S01E02, and S02E01 in the fixture,
        // inserted out of natural order to prove the sort.
        val eps = repo.listEpisodes(mediaId = 3L)
        assertEquals(3, eps.size)
        assertEquals(listOf(1 to 1, 1 to 2, 2 to 1), eps.map { it.seasonNumber to it.episodeNumber })
        assertEquals("Winter Is Coming", eps[0].title)
        assertEquals("The Kingsroad", eps[1].title)
        assertEquals("The North Remembers", eps[2].title)
    }

    @Test
    fun listEpisodes_carriesAllMetadataFields() {
        val first = repo.listEpisodes(mediaId = 3L).first()
        assertEquals("2011-04-17", first.airDate)
        assertEquals(62, first.runtimeMinutes)
        assertEquals(
            "https://image.tmdb.org/t/p/w300/winter.jpg",
            first.stillUrl,
        )
        assertNull("S01E01 has no cached still", first.stillPath)
        assertEquals(
            "Series/Game of Thrones [tmdb-1399]/Season 1/S01E01.Winter Is Coming.mkv",
            first.filePath,
        )
        assertNotNull(first.overview)
        assertTrue(first.overview!!.startsWith("Eddard Stark"))
    }

    @Test
    fun listEpisodes_toleratesNullableMetadata() {
        // S02E01 in the fixture has null overview and null still_*.
        val s02e01 = repo.listEpisodes(mediaId = 3L).single { it.seasonNumber == 2 }
        assertNull(s02e01.overview)
        assertNull(s02e01.stillPath)
        assertNull(s02e01.stillUrl)
    }

    @Test
    fun listRecentlyAddedMovies_returnsOnlyFlaggedRowsInsideWindow() {
        // 14-day window: only Inception qualifies (catalogued today,
        // is_new=1). Cidade de Deus is too old AND not flagged.
        // "Stale Flag" is flagged but catalogued 30 days ago, so it
        // must also be excluded — the filter is an AND, not an OR.
        val since = java.time.Instant.now()
            .minus(14, java.time.temporal.ChronoUnit.DAYS)
            .toString()
        val recent = repo.listRecentlyAddedMovies(since)
        assertEquals(1, recent.size)
        assertEquals("A Origem", recent.single().title)
    }

    @Test
    fun listRecentlyAddedMovies_returnsEmptyWithDistantCutoff() {
        // Cutoff in the year 9999 — no row's catalogued_at is greater.
        assertTrue(
            repo.listRecentlyAddedMovies("9999-01-01T00:00:00Z").isEmpty(),
        )
    }

    @Test
    fun mediaRow_carriesPosterFields() {
        val movie = repo.listMoviesByGenre(28L).single()
        assertNotNull(movie.posterPath)
        assertEquals("poster/27205.jpg", movie.posterPath)
        assertTrue(movie.posterUrl!!.startsWith("https://image.tmdb.org/"))
    }
}
