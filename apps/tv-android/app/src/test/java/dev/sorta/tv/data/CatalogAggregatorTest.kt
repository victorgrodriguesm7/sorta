package dev.sorta.tv.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class CatalogAggregatorTest {

    /** In-memory [Catalog] backed by hand-rolled rows for deterministic tests. */
    private class FakeCatalog(
        val genres: List<GenreRow> = emptyList(),
        val moviesByGenre: Map<Long, List<MediaRow>> = emptyMap(),
        val series: List<MediaRow> = emptyList(),
        val searchResults: Map<String, List<MediaRow>> = emptyMap(),
        val episodesByMediaId: Map<Long, List<EpisodeRow>> = emptyMap(),
        val recentlyAdded: List<MediaRow> = emptyList(),
        val settings: Map<String, String> = emptyMap(),
    ) : Catalog {
        var closed = false
        override fun listGenres(mediaType: MediaType?): List<GenreRow> =
            genres.filter { mediaType == null || it.mediaType == mediaType }
        override fun listMoviesByGenre(genreId: Long, primaryOnly: Boolean): List<MediaRow> =
            moviesByGenre[genreId].orEmpty()
        override fun listSeries(): List<MediaRow> = series
        override fun search(query: String): List<MediaRow> = searchResults[query].orEmpty()
        override fun listEpisodes(mediaId: Long): List<EpisodeRow> = episodesByMediaId[mediaId].orEmpty()
        override fun listRecentlyAddedMovies(sinceIsoTimestamp: String): List<MediaRow> = recentlyAdded
        override fun setting(key: String): String? = settings[key]
        override fun close() { closed = true }
    }

    private fun movie(
        id: Long,
        title: String,
        drive: String = "A",
        cataloguedAt: String? = null,
    ): MediaRow = MediaRow(
        id = id,
        tmdbId = id,
        mediaType = MediaType.MOVIE,
        title = title,
        originalTitle = null,
        runtimeMinutes = null,
        posterPath = null,
        posterUrl = null,
        folderPath = "Movies/$title",
        cataloguedAt = cataloguedAt,
        driveRoot = File("/storage/$drive"),
    )

    private fun series(id: Long, title: String, drive: String = "A"): MediaRow = movie(id, title, drive)
        .copy(mediaType = MediaType.TV, folderPath = "Series/$title")

    @Test
    fun listSeriesMergesAcrossDrivesAndSortsByTitle() {
        val a = FakeCatalog(series = listOf(series(1, "Breaking Bad", "A"), series(2, "Zoo", "A")))
        val b = FakeCatalog(series = listOf(series(3, "Andor", "B"), series(4, "Mad Men", "B")))
        val agg = CatalogAggregator(listOf(a, b))
        assertEquals(
            listOf("Andor", "Breaking Bad", "Mad Men", "Zoo"),
            agg.listSeries().map { it.title },
        )
    }

    @Test
    fun listMoviesByGenreFansOutAndSorts() {
        val a = FakeCatalog(moviesByGenre = mapOf(28L to listOf(movie(1, "Mad Max", "A"))))
        val b = FakeCatalog(moviesByGenre = mapOf(28L to listOf(movie(2, "Aliens", "B"))))
        val agg = CatalogAggregator(listOf(a, b))
        assertEquals(listOf("Aliens", "Mad Max"), agg.listMoviesByGenre(28L).map { it.title })
    }

    @Test
    fun listGenresDeduplicatesByIdAndTypeAndPrefersTranslatedName() {
        // Same TMDB genre on two drives — only one drive translated it.
        // The aggregated row should carry the translation regardless of
        // which drive came first in the locator's enumeration.
        val translated = GenreRow(id = 28, mediaType = MediaType.MOVIE, canonicalName = "Action", translatedName = "Ação")
        val plain = GenreRow(id = 28, mediaType = MediaType.MOVIE, canonicalName = "Action", translatedName = null)
        // First drive has only the plain version; second has the translation.
        // Expectation: aggregate prefers the translated one (any non-null wins).
        val plainFirst = CatalogAggregator(listOf(FakeCatalog(genres = listOf(plain)), FakeCatalog(genres = listOf(translated))))
        assertEquals(listOf("Ação"), plainFirst.listGenres(MediaType.MOVIE).map { it.displayName })
        // Order shouldn't matter.
        val translatedFirst = CatalogAggregator(listOf(FakeCatalog(genres = listOf(translated)), FakeCatalog(genres = listOf(plain))))
        assertEquals(listOf("Ação"), translatedFirst.listGenres(MediaType.MOVIE).map { it.displayName })
    }

    @Test
    fun listGenresOrdersByDisplayNameAcrossDrives() {
        val crime = GenreRow(28, MediaType.MOVIE, "Crime", null)
        val acao = GenreRow(99, MediaType.MOVIE, "Action", "Ação")
        val drama = GenreRow(18, MediaType.MOVIE, "Drama", null)
        val agg = CatalogAggregator(listOf(
            FakeCatalog(genres = listOf(crime)),
            FakeCatalog(genres = listOf(acao, drama)),
        ))
        assertEquals(listOf("Ação", "Crime", "Drama"), agg.listGenres(MediaType.MOVIE).map { it.displayName })
    }

    @Test
    fun listGenresFiltersByMediaType() {
        val movieGenre = GenreRow(28, MediaType.MOVIE, "Action", null)
        val tvGenre = GenreRow(18, MediaType.TV, "Drama", null)
        val agg = CatalogAggregator(listOf(FakeCatalog(genres = listOf(movieGenre, tvGenre))))
        assertEquals(listOf(MediaType.MOVIE), agg.listGenres(MediaType.MOVIE).map { it.mediaType })
        assertEquals(listOf(MediaType.TV), agg.listGenres(MediaType.TV).map { it.mediaType })
        assertEquals(setOf(MediaType.MOVIE, MediaType.TV), agg.listGenres().map { it.mediaType }.toSet())
    }

    @Test
    fun searchMergesAcrossDrivesAndSorts() {
        val a = FakeCatalog(searchResults = mapOf("mad" to listOf(movie(1, "Mad Men", "A"))))
        val b = FakeCatalog(searchResults = mapOf("mad" to listOf(movie(2, "Mad Max", "B"))))
        val agg = CatalogAggregator(listOf(a, b))
        assertEquals(listOf("Mad Max", "Mad Men"), agg.search("mad").map { it.title })
    }

    @Test
    fun listEpisodesReturnsFromAnyDriveThatHasTheId() {
        // Series IDs are per-drive; only one drive owns a given mediaId.
        // Aggregator simply concats — the empty drives contribute nothing.
        val ep = EpisodeRow(id = 1, mediaId = 5, seasonNumber = 1, episodeNumber = 1, title = "Pilot", overview = null, airDate = null, runtimeMinutes = null, stillPath = null, stillUrl = null, filePath = null)
        val agg = CatalogAggregator(listOf(
            FakeCatalog(),
            FakeCatalog(episodesByMediaId = mapOf(5L to listOf(ep))),
        ))
        assertEquals(listOf(ep), agg.listEpisodes(5L))
    }

    @Test
    fun listRecentlyAddedSortsByCataloguedAtDescending() {
        val older = movie(1, "Old", "A", cataloguedAt = "2026-05-01T00:00:00Z")
        val newer = movie(2, "New", "B", cataloguedAt = "2026-05-10T00:00:00Z")
        val nullStamp = movie(3, "Untagged", "B", cataloguedAt = null)
        val agg = CatalogAggregator(listOf(
            FakeCatalog(recentlyAdded = listOf(older)),
            FakeCatalog(recentlyAdded = listOf(newer, nullStamp)),
        ))
        // Newest-first; rows with no timestamp sink to the bottom.
        assertEquals(listOf("New", "Old", "Untagged"), agg.listRecentlyAddedMovies("0").map { it.title })
    }

    @Test
    fun settingReturnsFirstNonNullAcrossDrives() {
        val agg = CatalogAggregator(listOf(
            FakeCatalog(settings = emptyMap()),
            FakeCatalog(settings = mapOf("schema_version" to "4")),
        ))
        assertEquals("4", agg.setting("schema_version"))
        assertNull(agg.setting("absent_key"))
    }

    @Test
    fun closePropagatesToAllBackends() {
        val a = FakeCatalog()
        val b = FakeCatalog()
        CatalogAggregator(listOf(a, b)).close()
        assertTrue(a.closed)
        assertTrue(b.closed)
    }
}
