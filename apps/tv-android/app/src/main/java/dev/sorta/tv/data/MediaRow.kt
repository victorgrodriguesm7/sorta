package dev.sorta.tv.data

import java.io.File

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
    /**
     * ISO 8601 UTC timestamp (`2026-05-11T12:34:56Z`) the desktop
     * stamped when the row was inserted. Null on v3 drives that
     * predate the column — `MediaRepository` substitutes null
     * silently when reading older fixtures.
     */
    val cataloguedAt: String? = null,
    /**
     * "Mark as new" flag the user set at cataloging time. Drives
     * the Recently Added row on the browse screen.
     */
    val isNew: Boolean = false,
    /**
     * Drive this row was read from. Transient — not persisted in
     * `media`, populated by [MediaRepository.open] from the file's
     * parent directory. Lets callers resolve [posterPath] /
     * [folderPath] correctly when the browse UI is merging rows
     * from several HDs in one list. Null on rows constructed in
     * tests / fixtures where the drive doesn't matter.
     */
    val driveRoot: File? = null,
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
