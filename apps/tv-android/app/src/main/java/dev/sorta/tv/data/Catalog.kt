package dev.sorta.tv.data

/**
 * Read-side surface every catalog backend exposes. Extracted from
 * [MediaRepository] so the multi-drive [CatalogAggregator] can stand
 * in for "a single virtual catalog spanning N HDs" without callers
 * caring whether they're talking to one DB or several.
 *
 * Implementations:
 *   - [MediaRepository] — one HD, one `sorta.db`. The original.
 *   - [CatalogAggregator] — N HDs merged into a single virtual list.
 *   - Unit tests use a hand-rolled fake to dodge Android SQLite.
 *
 * Every method that returns rows promises the same ordering contract
 * the underlying [MediaRepository] queries do (typically by display
 * title) so callers can render them as-is.
 */
interface Catalog : java.io.Closeable {
    fun listGenres(mediaType: MediaType? = null): List<GenreRow>
    fun listMoviesByGenre(genreId: Long, primaryOnly: Boolean = true): List<MediaRow>
    fun listSeries(): List<MediaRow>
    fun search(query: String): List<MediaRow>
    fun listEpisodes(mediaId: Long): List<EpisodeRow>
    fun listRecentlyAddedMovies(sinceIsoTimestamp: String): List<MediaRow>
    fun setting(key: String): String?
}
