package dev.sorta.tv.data

/**
 * Compatibility check between the on-disk Sorta schema and the
 * version this build of the reader was compiled against.
 *
 * Per `docs/disk-format.md#schema-versioning`:
 *   - on_disk > known  → refuse to open; user must update the reader.
 *   - on_disk == known → fine.
 *   - on_disk < known  → tolerated; the older desktop wrote less, but
 *                        nothing we know how to read has gone away.
 */
object SchemaCompat {

    /**
     * The latest on-disk schema this reader build understands. Bumped
     * in lockstep with the desktop's `CURRENT_SCHEMA_VERSION`. Version
     * 4 adds:
     *   - `media.is_new` + `media.catalogued_at`
     *   - the `episodes` table (per-episode TMDB metadata + stills)
     *
     * v3 drives keep opening — the new columns/table are read with
     * graceful fallbacks (see `MediaRepository`).
     */
    const val KNOWN_SCHEMA_VERSION: Int = 4

    sealed interface Result {
        /** Versions match exactly — proceed. */
        data object Ok : Result

        /**
         * On-disk version predates the reader's known version. We can
         * still open it; some optional fields may be missing.
         */
        data class OnDiskOlderTolerated(val onDisk: Int, val known: Int) : Result

        /**
         * On-disk version is newer than the reader knows about.
         * Refuse to open and tell the user to update.
         */
        data class OnDiskNewer(val onDisk: Int, val known: Int) : Result
    }

    fun isCompatible(onDisk: Int, known: Int = KNOWN_SCHEMA_VERSION): Result = when {
        onDisk > known -> Result.OnDiskNewer(onDisk, known)
        onDisk < known -> Result.OnDiskOlderTolerated(onDisk, known)
        else -> Result.Ok
    }

    /** Convenience: should we refuse to open? */
    fun Result.shouldRefuse(): Boolean = this is Result.OnDiskNewer
}
