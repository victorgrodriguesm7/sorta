package dev.sorta.tv.data

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper

/**
 * Local watch-state store. Lives in the app's internal storage —
 * never on the catalogued drive (the disk-format spec mandates the
 * reader stays read-only on the drive).
 *
 * Keys are paths **relative to the drive root** (e.g.
 * `Movies/Ação/Anônimo 2 [tmdb-1007734]/Anônimo 2 [tmdb-1007734].mkv`).
 * That makes state survive the drive being plugged into another box
 * and even the box being reflashed, as long as the user doesn't
 * rearrange the catalog manually.
 *
 * Schema:
 *
 *   watch_history (
 *     media_key    TEXT PRIMARY KEY,   -- relative path under drive root
 *     position_ms  INTEGER NOT NULL,   -- last known playback position
 *     duration_ms  INTEGER NOT NULL,   -- last known total duration (0 if unknown)
 *     watched      INTEGER NOT NULL,   -- 0 / 1 — explicit completion flag
 *     updated_at   INTEGER NOT NULL    -- epoch milliseconds
 *   )
 */
class WatchHistory internal constructor(
    private val helper: SQLiteOpenHelper,
) {

    data class Progress(
        val positionMs: Long,
        val durationMs: Long,
        val watched: Boolean,
    ) {
        /** Float in [0, 1]; 0 if duration is unknown. */
        val fraction: Float
            get() = if (durationMs > 0) (positionMs.toFloat() / durationMs).coerceIn(0f, 1f) else 0f
    }

    /**
     * Record a fresh playback observation. If [positionMs] reaches at
     * least [AUTO_WATCHED_FRACTION] of [durationMs], the row is marked
     * watched automatically — players that report a near-end position
     * on exit usually mean "credits are rolling".
     */
    fun record(mediaKey: String, positionMs: Long, durationMs: Long) {
        val now = System.currentTimeMillis()
        val watched = durationMs > 0 &&
            positionMs.toDouble() / durationMs >= AUTO_WATCHED_FRACTION
        helper.writableDatabase.insertWithOnConflict(
            TABLE,
            null,
            ContentValues().apply {
                put("media_key", mediaKey)
                put("position_ms", positionMs)
                put("duration_ms", durationMs)
                put("watched", if (watched) 1 else 0)
                put("updated_at", now)
            },
            SQLiteDatabase.CONFLICT_REPLACE,
        )
    }

    /** Force the watched flag without touching position / duration. */
    fun setWatched(mediaKey: String, watched: Boolean) {
        val now = System.currentTimeMillis()
        helper.writableDatabase.execSQL(
            """
            INSERT INTO $TABLE (media_key, position_ms, duration_ms, watched, updated_at)
            VALUES (?, 0, 0, ?, ?)
            ON CONFLICT(media_key) DO UPDATE SET watched = excluded.watched, updated_at = excluded.updated_at
            """.trimIndent(),
            arrayOf<Any>(mediaKey, if (watched) 1 else 0, now),
        )
    }

    /** Single-row lookup. Returns null if there's no record yet. */
    fun progressFor(mediaKey: String): Progress? {
        helper.readableDatabase.rawQuery(
            "SELECT position_ms, duration_ms, watched FROM $TABLE WHERE media_key = ?",
            arrayOf(mediaKey),
        ).use { c ->
            return if (c.moveToNext()) {
                Progress(
                    positionMs = c.getLong(0),
                    durationMs = c.getLong(1),
                    watched = c.getInt(2) == 1,
                )
            } else null
        }
    }

    /**
     * Bulk lookup: every row whose `media_key` starts with [prefix]
     * (a relative folder path, e.g. `Series/Show [tmdb-9]`). Returns
     * a map keyed by the **full** media_key.
     */
    fun progressUnder(prefix: String): Map<String, Progress> {
        val pattern = prefix.trimEnd('/') + "/%"
        val out = HashMap<String, Progress>()
        helper.readableDatabase.rawQuery(
            "SELECT media_key, position_ms, duration_ms, watched FROM $TABLE WHERE media_key LIKE ? ESCAPE '\\'",
            arrayOf(pattern.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%"),
        ).use { c ->
            while (c.moveToNext()) {
                out[c.getString(0)] = Progress(
                    positionMs = c.getLong(1),
                    durationMs = c.getLong(2),
                    watched = c.getInt(3) == 1,
                )
            }
        }
        return out
    }

    fun close() {
        helper.close()
    }

    private class Helper(context: Context) : SQLiteOpenHelper(context, DB_NAME, null, DB_VERSION) {
        override fun onCreate(db: SQLiteDatabase) {
            db.execSQL(
                """
                CREATE TABLE $TABLE (
                    media_key   TEXT PRIMARY KEY,
                    position_ms INTEGER NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    watched     INTEGER NOT NULL,
                    updated_at  INTEGER NOT NULL
                )
                """.trimIndent(),
            )
        }

        override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
            // No upgrades yet. When we bump DB_VERSION, add migrations here.
        }
    }

    companion object {
        private const val DB_NAME = "watch_history.db"
        private const val DB_VERSION = 1
        private const val TABLE = "watch_history"
        const val AUTO_WATCHED_FRACTION = 0.95

        @Volatile private var instance: WatchHistory? = null

        fun get(context: Context): WatchHistory =
            instance ?: synchronized(this) {
                instance ?: WatchHistory(Helper(context.applicationContext)).also { instance = it }
            }

        /**
         * Build the canonical key for a video file given the drive
         * root that owns it. Keys are forward-slash-only so the same
         * row is found whether the file was discovered via direct
         * path access or through the SAF fallback.
         */
        fun keyFor(driveRoot: java.io.File, file: java.io.File): String {
            val rootPath = driveRoot.absolutePath.trimEnd('/')
            val filePath = file.absolutePath.replace('\\', '/')
            val rootNormalized = rootPath.replace('\\', '/')
            return if (filePath.startsWith("$rootNormalized/")) {
                filePath.substring(rootNormalized.length + 1)
            } else {
                filePath
            }
        }
    }
}
