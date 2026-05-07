package dev.sorta.tv.data

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
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
        assertEquals("3", repo.setting("schema_version"))
        assertEquals("Movies", repo.setting("movies_folder_label"))
        assertEquals("Season", repo.setting("season_label"))
        assertNull(repo.setting("nonexistent_key"))
    }

    @Test
    fun mediaRow_carriesPosterFields() {
        val movie = repo.listMoviesByGenre(28L).single()
        assertNotNull(movie.posterPath)
        assertEquals("poster/27205.jpg", movie.posterPath)
        assertTrue(movie.posterUrl!!.startsWith("https://image.tmdb.org/"))
    }
}
