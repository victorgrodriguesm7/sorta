package dev.sorta.tv.data

/**
 * One row from the `media` table. Mirrors the columns documented in
 * `docs/disk-format.md#media`.
 */
data class MediaRow(
    val id: Long,
    val tmdbId: Long,
    val mediaType: MediaType,
    val title: String,
    val originalTitle: String?,
    val runtimeMinutes: Int?,
    /** Path relative to the HD root, e.g. `poster/27205.jpg`. */
    val posterPath: String?,
    /** TMDB CDN fallback when the local poster file is missing. */
    val posterUrl: String?,
    /** Path relative to the HD root, e.g. `Movies/Action/X [tmdb-1]`. */
    val folderPath: String,
)

enum class MediaType(val sqlValue: String) {
    MOVIE("movie"),
    TV("tv");

    companion object {
        fun fromSql(value: String): MediaType = when (value) {
            "movie" -> MOVIE
            "tv" -> TV
            else -> error("unknown media_type: $value")
        }
    }
}
