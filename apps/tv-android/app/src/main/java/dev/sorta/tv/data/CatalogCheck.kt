package dev.sorta.tv.data

import dev.sorta.tv.usb.UsbDriveLocator
import java.io.File

/**
 * Single decision point for "is this drive ready to browse?". Used
 * by [dev.sorta.tv.ui.BrowseActivity] to choose between rendering the
 * Leanback browse UI and the error fragment.
 *
 * Pure-ish: takes the storage root as a parameter so JVM tests can
 * exercise it under TemporaryFolder.
 */
object CatalogCheck {

    sealed interface Result {
        data class Ok(val driveRoot: File) : Result
        data object NoDrive : Result
        data class MissingDb(val driveRoot: File) : Result
        data class SchemaTooNew(val onDisk: Int, val known: Int) : Result
    }

    /**
     * @param storageRoot directory we look for plugged-in drives under
     * @param openRepository factory that opens [MediaRepository]
     *        against a `sorta.db` file. Injectable so tests can hand
     *        in a fake without spinning up Android SQLite.
     */
    fun run(
        storageRoot: File = UsbDriveLocator.DEFAULT_STORAGE_ROOT,
        openRepository: (File) -> MediaRepository = MediaRepository::open,
    ): Result {
        val drive = UsbDriveLocator.locate(storageRoot).firstOrNull() ?: return Result.NoDrive
        val dbFile = File(drive, "sorta.db")
        if (!dbFile.isFile) return Result.MissingDb(drive)
        val onDisk = openRepository(dbFile).use { repo ->
            repo.setting("schema_version")?.toIntOrNull() ?: 0
        }
        val compat = SchemaCompat.isCompatible(onDisk)
        return when (compat) {
            is SchemaCompat.Result.OnDiskNewer ->
                Result.SchemaTooNew(compat.onDisk, compat.known)
            is SchemaCompat.Result.Ok,
            is SchemaCompat.Result.OnDiskOlderTolerated -> Result.Ok(drive)
        }
    }
}
