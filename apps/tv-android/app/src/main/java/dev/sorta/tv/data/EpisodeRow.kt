package dev.sorta.tv.data

/**
 * One row from the v4 `episodes` table. Mirrors the columns the
 * desktop writes via `link_as_series` / `recatalog_series` (see
 * `apps/desktop/src-tauri/migrations/0004_episodes_and_flags.sql`).
 *
 * Drives whose desktop binary predates v4 don't have this table at
 * all — `MediaRepository.listEpisodes` returns an empty list rather
 * than crashing on v3 fixtures, and callers fall back to
 * `SeriesScanner` for filesystem-derived metadata.
 *
 * `stillPath` / `filePath` are stored relative to the HD root, exactly
 * like `MediaRow.posterPath` and `MediaRow.folderPath`. The desktop on
 * Windows writes them with `\` separators; we normalize to `/` at
 * read time so callers only ever see POSIX paths.
 */
data class EpisodeRow(
    val id: Long,
    val mediaId: Long,
    val seasonNumber: Int,
    val episodeNumber: Int,
    val title: String?,
    val overview: String?,
    /** ISO YYYY-MM-DD, NULL when TMDB doesn't have one yet. */
    val airDate: String?,
    val runtimeMinutes: Int?,
    /** Cached still image, relative to the HD root. */
    val stillPath: String?,
    /** TMDB CDN fallback URL. */
    val stillUrl: String?,
    /** Video file, relative to the HD root. */
    val filePath: String?,
)
