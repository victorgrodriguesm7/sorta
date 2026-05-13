package dev.sorta.tv.data

import org.json.JSONException
import org.json.JSONObject

/**
 * `manifest.json` — the small companion file Sorta writes next to
 * `sorta.db`. See `docs/disk-format.md`. We tolerate unknown keys per
 * spec, and treat a missing file as a non-error elsewhere.
 */
data class Manifest(
    val schemaVersion: Int,
    val appVersion: String?,
    val generatedAt: String?,
    val counts: Counts,
) {
    data class Counts(
        val mediaTotal: Int,
        val movies: Int,
        val series: Int,
    )

    companion object {
        /**
         * Parse the manifest from raw JSON text.
         *
         * Throws [IllegalArgumentException] when the document isn't a
         * valid JSON object or when `schema_version` is missing /
         * non-numeric / negative — the only field we treat as required.
         * All other fields fall back to safe defaults.
         */
        fun fromJson(text: String): Manifest {
            val root = try {
                JSONObject(text)
            } catch (e: JSONException) {
                throw IllegalArgumentException("manifest.json is not a JSON object", e)
            }

            val schemaVersion = if (root.has("schema_version")) {
                val v = root.opt("schema_version")
                (v as? Number)?.toInt()
                    ?: throw IllegalArgumentException("schema_version must be an integer, got $v")
            } else {
                throw IllegalArgumentException("schema_version is required")
            }
            require(schemaVersion >= 0) { "schema_version must be non-negative, got $schemaVersion" }

            val appVersion = root.optString("app_version", "").ifEmpty { null }
            val generatedAt = root.optString("generated_at", "").ifEmpty { null }

            val countsObj = root.optJSONObject("counts")
            val counts = if (countsObj != null) {
                Counts(
                    mediaTotal = countsObj.optInt("media_total", 0),
                    movies = countsObj.optInt("movies", 0),
                    series = countsObj.optInt("series", 0),
                )
            } else {
                Counts(0, 0, 0)
            }

            return Manifest(schemaVersion, appVersion, generatedAt, counts)
        }
    }
}
