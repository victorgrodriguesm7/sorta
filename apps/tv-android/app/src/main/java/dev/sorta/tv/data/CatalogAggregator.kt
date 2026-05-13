package dev.sorta.tv.data

/**
 * Single [Catalog] facade over multiple HD-scoped [Catalog] backends.
 * Each `sorta.db` the user has plugged in becomes one backend; this
 * class fans queries out to all of them and merges the results.
 *
 * Merge rules:
 *   - [listSeries] / [listMoviesByGenre] / [search] / [listRecentlyAddedMovies]:
 *     concat all backends, then re-sort by the same key the underlying
 *     [MediaRepository] queries already use (display title, or
 *     `cataloguedAt` descending for the recently-added row).
 *   - [listGenres]: TMDB genre IDs are global across drives, so we
 *     dedupe by `(id, mediaType)`. When the same genre appears on
 *     several drives we keep the first non-null `translatedName` —
 *     a single drive with a translation always wins, regardless of
 *     locator enumeration order.
 *   - [listEpisodes]: `media.id` is per-drive, so at most one backend
 *     actually owns a given id. Empty fan-outs cost nothing.
 *   - [setting]: returns the first non-null value across backends.
 *     Used by call sites that don't care which drive answered.
 *
 * No duplicate-movie handling: per the multi-drive spec the user
 * guarantees a given catalog row exists on exactly one drive.
 *
 * Thread-safety: matches [MediaRepository] — caller serialises.
 */
class CatalogAggregator(
    private val backends: List<Catalog>,
) : Catalog {

    override fun listGenres(mediaType: MediaType?): List<GenreRow> {
        // Dedupe across drives by (id, mediaType). When two drives
        // disagree on `translatedName`, prefer any non-null value so
        // a single translated drive lights up the genre name for the
        // whole catalog.
        val byKey = LinkedHashMap<Pair<Long, MediaType>, GenreRow>()
        backends.asSequence()
            .flatMap { it.listGenres(mediaType).asSequence() }
            .forEach { row ->
                val key = row.id to row.mediaType
                val existing = byKey[key]
                byKey[key] = when {
                    existing == null -> row
                    existing.translatedName == null && row.translatedName != null -> row
                    else -> existing
                }
            }
        return byKey.values.sortedBy { it.displayName.lowercase() }
    }

    override fun listMoviesByGenre(genreId: Long, primaryOnly: Boolean): List<MediaRow> =
        backends.flatMap { it.listMoviesByGenre(genreId, primaryOnly) }
            .sortedBy { it.title.lowercase() }

    override fun listSeries(): List<MediaRow> =
        backends.flatMap { it.listSeries() }.sortedBy { it.title.lowercase() }

    override fun search(query: String): List<MediaRow> =
        backends.flatMap { it.search(query) }.sortedBy { it.title.lowercase() }

    override fun listEpisodes(mediaId: Long): List<EpisodeRow> =
        backends.flatMap { it.listEpisodes(mediaId) }

    override fun listRecentlyAddedMovies(sinceIsoTimestamp: String): List<MediaRow> {
        // ISO 8601 second-precision sorts lexicographically the same
        // way as chronologically, so a string compare is fine. We use
        // `nullsFirst` so that under `compareByDescending` (which
        // negates the comparator) nulls sink to the bottom — they
        // shouldn't appear in the recently-added bucket at all, but
        // if they do we don't want them above real timestamps.
        return backends.flatMap { it.listRecentlyAddedMovies(sinceIsoTimestamp) }
            .sortedWith(compareByDescending(nullsFirst<String>()) { it.cataloguedAt })
    }

    override fun setting(key: String): String? =
        backends.firstNotNullOfOrNull { it.setting(key) }

    override fun close() {
        // Close every backend even if one throws; report the first
        // failure so the caller still sees it.
        var thrown: Throwable? = null
        for (b in backends) {
            try { b.close() } catch (t: Throwable) {
                if (thrown == null) thrown = t
            }
        }
        thrown?.let { throw it }
    }
}
