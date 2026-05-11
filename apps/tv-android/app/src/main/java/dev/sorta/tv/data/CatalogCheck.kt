package dev.sorta.tv.data

import dev.sorta.tv.usb.UsbDriveLocator
import java.io.File

/**
 * Single decision point for "which drives can we browse together?".
 * Used by [dev.sorta.tv.ui.BrowseActivity] to choose between rendering
 * the Leanback browse UI (one or more compatible drives) and the
 * error fragment (no drive at all, or every drive newer than this
 * build understands).
 *
 * Multi-drive policy:
 *   - All drives whose `settings.schema_version` equals the **maximum**
 *     value found across plugged-in drives are loaded together.
 *   - Older drives at a lower version are intentionally hidden.
 *     Mixing schemas across the merged catalog would force every query
 *     into the lowest-common-denominator shape (no `episodes` table,
 *     no `is_new`/`catalogued_at`), and the user explicitly opted out
 *     of that — V3 + V3 + V5 must load only the V5.
 *   - The maximum version is compared against [SchemaCompat] exactly
 *     like the single-drive path used to: only a value newer than
 *     this build returns [Result.SchemaTooNew].
 *
 * Pure-ish: takes the storage root + schema-version reader as
 * parameters so JVM tests can exercise it under `TemporaryFolder`
 * without spinning up Android SQLite.
 */
object CatalogCheck {

    sealed interface Result {
        /**
         * One or more drives at the same compatible schema version.
         * The list preserves locator ordering.
         */
        data class Ok(val driveRoots: List<File>, val schemaVersion: Int) : Result

        /** No drives with a `sorta.db` were found at all. */
        data object NoDrive : Result

        /**
         * A drive was plugged in but had no `sorta.db` — currently
         * unreachable because [UsbDriveLocator] already filters on
         * that file, but kept in the type for future locator
         * relaxation and so the existing error UI keeps compiling.
         */
        data class MissingDb(val driveRoot: File) : Result

        /**
         * The highest version present on any plugged-in drive exceeds
         * what this build of the reader knows how to parse. Refuse to
         * load — see [SchemaCompat] for the rationale.
         */
        data class SchemaTooNew(val onDisk: Int, val known: Int) : Result
    }

    /**
     * @param storageRoot directory we look for plugged-in drives under
     * @param known       reader's compiled-in schema version
     * @param readSchemaVersion factory that reads the `schema_version`
     *        setting from a given `sorta.db` file. Null means the
     *        drive's `settings` row is missing — treated as version 0
     *        so it loses out to any real versioned drive.
     */
    fun run(
        storageRoot: File = UsbDriveLocator.DEFAULT_STORAGE_ROOT,
        known: Int = SchemaCompat.KNOWN_SCHEMA_VERSION,
        readSchemaVersion: (File) -> Int? = defaultSchemaReader,
    ): Result {
        val drives = UsbDriveLocator.locate(storageRoot)
        if (drives.isEmpty()) return Result.NoDrive

        // (driveRoot, version) for every plugged-in catalog. Missing
        // schema_version → 0 so it sorts under any tagged drive.
        val versioned = drives.map { drive ->
            val version = readSchemaVersion(File(drive, "sorta.db")) ?: 0
            drive to version
        }
        val maxVersion = versioned.maxOf { it.second }

        if (maxVersion > known) {
            return Result.SchemaTooNew(onDisk = maxVersion, known = known)
        }

        // Keep only the drives at the winning version — V3+V3+V5 → V5,
        // V3+V3 → both V3s.
        val winners = versioned.filter { it.second == maxVersion }.map { it.first }
        return Result.Ok(winners, maxVersion)
    }

    /** Default reader — opens the DB read-only and pulls `settings.schema_version`. */
    private val defaultSchemaReader: (File) -> Int? = { dbFile ->
        MediaRepository.open(dbFile).use { it.setting("schema_version")?.toIntOrNull() }
    }
}
