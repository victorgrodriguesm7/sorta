package dev.sorta.tv.data

import android.database.sqlite.SQLiteDatabase
import java.io.Closeable
import java.io.File

/**
 * Read-only typed access to a `sorta.db` shipped on a user's hard
 * drive. The desktop is the only writer; this app opens with
 * `OPEN_READONLY` so a misbehaving query can't damage the catalog.
 *
 * Usage:
 *
 *     MediaRepository.open(File("/storage/usb1/sorta.db")).use { repo ->
 *         val movieGenres = repo.listGenres(MediaType.MOVIE)
 *         val movies      = repo.listMoviesByGenre(genreId = 28L)
 *         val series      = repo.listSeries()
 *     }
 */
class MediaRepository internal constructor(
    private val db: SQLiteDatabase,
) : Closeable {

    /**
     * Genres known on this drive, optionally filtered to one media
     * type. Sorted by display name (the user's translation, falling
     * back to canonical English) so callers can render rows in order.
     */
    fun listGenres(mediaType: MediaType? = null): List<GenreRow> {
        val (where, args) = if (mediaType != null) {
            "WHERE media_type = ?" to arrayOf(mediaType.sqlValue)
        } else {
            "" to emptyArray()
        }
        val sql = """
            SELECT id, media_type, canonical_name, translated_name
            FROM genres
            $where
            ORDER BY COALESCE(translated_name, canonical_name) COLLATE NOCASE
        """.trimIndent()
        return db.rawQuery(sql, args).use { c ->
            buildList {
                while (c.moveToNext()) {
                    add(
                        GenreRow(
                            id = c.getLong(0),
                            mediaType = MediaType.fromSql(c.getString(1)),
                            canonicalName = c.getString(2),
                            translatedName = c.getStringOrNull(3),
                        )
                    )
                }
            }
        }
    }

    /**
     * Movies catalogued under [genreId]. By default returns only the
     * movies whose primary genre is [genreId] — the same convention
     * the desktop uses to pick the on-disk folder, so each movie shows
     * up exactly once in the browse UI. Pass [primaryOnly] = false to
     * get every match, including secondary-genre links.
     *
     * Sorted by display title.
     */
    fun listMoviesByGenre(genreId: Long, primaryOnly: Boolean = true): List<MediaRow> {
        val primaryClause = if (primaryOnly) "AND mg.is_primary = 1" else ""
        val sql = """
            SELECT m.id, m.tmdb_id, m.media_type, m.title, m.original_title,
                   m.runtime_minutes, m.poster_path, m.poster_url, m.folder_path,
                   m.catalogued_at, m.is_new
            FROM media m
            INNER JOIN media_genres mg ON mg.media_id = m.id
            WHERE m.media_type = 'movie' AND mg.genre_id = ?
              AND mg.media_type = 'movie' $primaryClause
            ORDER BY m.title COLLATE NOCASE
        """.trimIndent()
        return db.rawQuery(sql, arrayOf(genreId.toString())).use { it.toMediaRows() }
    }

    /** Every linked series, sorted by display title. */
    fun listSeries(): List<MediaRow> {
        val sql = """
            SELECT id, tmdb_id, media_type, title, original_title,
                   runtime_minutes, poster_path, poster_url, folder_path,
                   catalogued_at, is_new
            FROM media
            WHERE media_type = 'tv'
            ORDER BY title COLLATE NOCASE
        """.trimIndent()
        return db.rawQuery(sql, emptyArray()).use { it.toMediaRows() }
    }

    /**
     * Case-insensitive substring match against `title` and
     * `original_title`. Returns rows of either media type, sorted by
     * display title. Empty / blank queries return an empty list.
     */
    fun search(query: String): List<MediaRow> {
        if (query.isBlank()) return emptyList()
        val pattern = "%${query.trim().sqlLikeEscape()}%"
        val sql = """
            SELECT id, tmdb_id, media_type, title, original_title,
                   runtime_minutes, poster_path, poster_url, folder_path,
                   catalogued_at, is_new
            FROM media
            WHERE title LIKE ? ESCAPE '\'
               OR original_title LIKE ? ESCAPE '\'
            ORDER BY title COLLATE NOCASE
        """.trimIndent()
        return db.rawQuery(sql, arrayOf(pattern, pattern)).use { it.toMediaRows() }
    }

    /**
     * Every episode of a series, sorted by `(season_number, episode_number)`.
     * Returns an empty list when the `episodes` table is missing (v3
     * drive) or when the row simply has no episode metadata yet —
     * callers handle both cases by falling back to a filesystem walk.
     */
    fun listEpisodes(mediaId: Long): List<EpisodeRow> {
        val sql = """
            SELECT id, media_id, season_number, episode_number,
                   title, overview, air_date, runtime_minutes,
                   still_path, still_url, file_path
            FROM episodes
            WHERE media_id = ?
            ORDER BY season_number, episode_number
        """.trimIndent()
        return try {
            db.rawQuery(sql, arrayOf(mediaId.toString())).use { it.toEpisodeRows() }
        } catch (e: android.database.sqlite.SQLiteException) {
            // `no such table: episodes` on pre-v4 drives. Surface as
            // empty rather than crashing the UI.
            emptyList()
        }
    }

    /**
     * Movies that:
     *   - have `is_new = 1` (user flagged them at cataloging time), AND
     *   - were catalogued at or after [sinceIsoTimestamp] (e.g. now − 14d).
     *
     * Both conditions are AND-ed — flipping `is_new` on an older row
     * doesn't drag it back into the recently-added bucket. Returns an
     * empty list on v3 drives that don't have the columns.
     */
    fun listRecentlyAddedMovies(sinceIsoTimestamp: String): List<MediaRow> {
        val sql = """
            SELECT id, tmdb_id, media_type, title, original_title,
                   runtime_minutes, poster_path, poster_url, folder_path,
                   catalogued_at, is_new
            FROM media
            WHERE media_type = 'movie'
              AND is_new = 1
              AND catalogued_at >= ?
            ORDER BY catalogued_at DESC
        """.trimIndent()
        return try {
            db.rawQuery(sql, arrayOf(sinceIsoTimestamp)).use { it.toMediaRows() }
        } catch (e: android.database.sqlite.SQLiteException) {
            emptyList()
        }
    }

    /** Read a single `settings` value, or null if the key is unknown. */
    fun setting(key: String): String? {
        return db.rawQuery("SELECT value FROM settings WHERE key = ?", arrayOf(key)).use { c ->
            if (c.moveToNext()) c.getString(0) else null
        }
    }

    override fun close() {
        db.close()
    }

    companion object {
        /**
         * Open a `sorta.db` file read-only. Caller owns the returned
         * repository and must `close()` it.
         */
        fun open(dbFile: File): MediaRepository {
            val db = SQLiteDatabase.openDatabase(
                dbFile.absolutePath,
                /* factory = */ null,
                SQLiteDatabase.OPEN_READONLY,
            )
            return MediaRepository(db)
        }
    }
}

/** Escape `%`, `_`, and `\` so a user query can't break out of LIKE. */
private fun String.sqlLikeEscape(): String =
    replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")

private fun android.database.Cursor.getStringOrNull(index: Int): String? =
    if (isNull(index)) null else getString(index)

/**
 * The desktop on Windows currently writes `\` separators into
 * `media.folder_path` / `media.poster_path` — a violation of
 * docs/disk-format.md ("path relative to HD root, e.g.
 * `poster/27205.jpg`" — forward slashes). Normalize here so the
 * rest of the reader code only ever sees POSIX paths.
 */
private fun String?.posixPath(): String? = this?.replace('\\', '/')

private fun android.database.Cursor.toMediaRows(): List<MediaRow> = use { c ->
    // catalogued_at + is_new were added in v4. v3 drives don't select
    // them (the queries here all reference them, but if a caller
    // hand-rolls a SELECT without those columns the indices will be
    // out of range — guard with column lookups instead of positional
    // reads).
    val cataloguedAtIdx = c.getColumnIndex("catalogued_at")
    val isNewIdx = c.getColumnIndex("is_new")
    buildList {
        while (c.moveToNext()) {
            add(
                MediaRow(
                    id = c.getLong(0),
                    tmdbId = c.getLong(1),
                    mediaType = MediaType.fromSql(c.getString(2)),
                    title = c.getString(3),
                    originalTitle = c.getStringOrNull(4),
                    runtimeMinutes = if (c.isNull(5)) null else c.getInt(5),
                    posterPath = c.getStringOrNull(6).posixPath(),
                    posterUrl = c.getStringOrNull(7),
                    folderPath = c.getString(8).posixPath()!!,
                    cataloguedAt = if (cataloguedAtIdx >= 0 && !c.isNull(cataloguedAtIdx)) {
                        c.getString(cataloguedAtIdx).takeIf { it.isNotEmpty() }
                    } else null,
                    isNew = if (isNewIdx >= 0 && !c.isNull(isNewIdx)) {
                        c.getInt(isNewIdx) != 0
                    } else false,
                )
            )
        }
    }
}

private fun android.database.Cursor.toEpisodeRows(): List<EpisodeRow> = use { c ->
    buildList {
        while (c.moveToNext()) {
            add(
                EpisodeRow(
                    id = c.getLong(0),
                    mediaId = c.getLong(1),
                    seasonNumber = c.getInt(2),
                    episodeNumber = c.getInt(3),
                    title = c.getStringOrNull(4),
                    overview = c.getStringOrNull(5),
                    airDate = c.getStringOrNull(6),
                    runtimeMinutes = if (c.isNull(7)) null else c.getInt(7),
                    stillPath = c.getStringOrNull(8).posixPath(),
                    stillUrl = c.getStringOrNull(9),
                    filePath = c.getStringOrNull(10).posixPath(),
                )
            )
        }
    }
}
