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
     * Movies catalogued under [genreId] (matching against `media_genres`,
     * any role — primary or secondary). Sorted by display title.
     */
    fun listMoviesByGenre(genreId: Long): List<MediaRow> {
        val sql = """
            SELECT m.id, m.tmdb_id, m.media_type, m.title, m.original_title,
                   m.runtime_minutes, m.poster_path, m.poster_url, m.folder_path
            FROM media m
            INNER JOIN media_genres mg ON mg.media_id = m.id
            WHERE m.media_type = 'movie' AND mg.genre_id = ? AND mg.media_type = 'movie'
            ORDER BY m.title COLLATE NOCASE
        """.trimIndent()
        return db.rawQuery(sql, arrayOf(genreId.toString())).use { it.toMediaRows() }
    }

    /** Every linked series, sorted by display title. */
    fun listSeries(): List<MediaRow> {
        val sql = """
            SELECT id, tmdb_id, media_type, title, original_title,
                   runtime_minutes, poster_path, poster_url, folder_path
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
                   runtime_minutes, poster_path, poster_url, folder_path
            FROM media
            WHERE title LIKE ? ESCAPE '\'
               OR original_title LIKE ? ESCAPE '\'
            ORDER BY title COLLATE NOCASE
        """.trimIndent()
        return db.rawQuery(sql, arrayOf(pattern, pattern)).use { it.toMediaRows() }
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

private fun android.database.Cursor.toMediaRows(): List<MediaRow> = use { c ->
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
                    posterPath = c.getStringOrNull(6),
                    posterUrl = c.getStringOrNull(7),
                    folderPath = c.getString(8),
                )
            )
        }
    }
}
