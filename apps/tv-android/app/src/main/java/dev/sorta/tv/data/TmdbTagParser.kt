package dev.sorta.tv.data

/**
 * Parses the `[tmdb-{id}]` suffix Sorta puts on every catalogued folder
 * (and renamed video file). Mirrors the desktop's `parse_tmdb_id` /
 * `strip_tmdb_tag` / `is_catalogued_folder` so external readers can stay
 * in sync without reading Rust source.
 */
object TmdbTagParser {

    private val TAG_REGEX = Regex("""\[tmdb-(\d+)]\s*$""")

    /** Extract the numeric TMDB id from a folder/file name, or null. */
    fun parseTmdbId(name: String): Long? {
        val trimmed = name.trim()
        val match = TAG_REGEX.find(trimmed) ?: return null
        return match.groupValues[1].toLongOrNull()
    }

    /**
     * Strip the trailing `[tmdb-{id}]` suffix and return the bare title.
     * Returns null if the input doesn't match the catalogued convention,
     * or if the title prefix is empty.
     */
    fun stripTmdbTag(name: String): String? {
        val trimmed = name.trimEnd()
        val match = TAG_REGEX.find(trimmed) ?: return null
        val prefix = trimmed.substring(0, match.range.first).trimEnd()
        return prefix.ifEmpty { null }
    }

    /**
     * True if the folder name matches the convention
     * `<non-empty title> [tmdb-{id}]` (trailing whitespace tolerated).
     */
    fun isCataloguedFolder(name: String): Boolean = stripTmdbTag(name) != null
}
