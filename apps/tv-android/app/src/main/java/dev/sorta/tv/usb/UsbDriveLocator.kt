package dev.sorta.tv.usb

import java.io.File

/**
 * Finds plugged-in drives that look like a Sorta-catalogued HD.
 *
 * On API 25 (the deployment target) USB mass-storage volumes appear
 * under `/storage/usb<HHHH>/` and are world-readable. We walk that
 * directory and pick out children that contain a `sorta.db` at their
 * root — that's the catalog file the desktop writes.
 *
 * SAF fallback (for stricter vendor builds where `/storage/` is
 * denied) is layered on top of this in `usb/SafFallback.kt`.
 *
 * Pure-ish: the entry point takes the storage root as a parameter so
 * unit tests can hand in a temp directory.
 */
object UsbDriveLocator {

    /** Default storage prefix on Android. */
    val DEFAULT_STORAGE_ROOT: File = File("/storage")

    /**
     * Return every direct child of [storageRoot] that looks like a
     * Sorta-catalogued drive — i.e. contains a readable `sorta.db`
     * file at its root. Internal storage (`emulated`, `self`) and the
     * `/storage/` parent itself are skipped.
     */
    fun locate(storageRoot: File = DEFAULT_STORAGE_ROOT): List<File> {
        val children = storageRoot.listFiles() ?: return emptyList()
        return children
            .asSequence()
            .filter { it.isDirectory }
            .filter { !isInternalStorage(it.name) }
            .filter { File(it, "sorta.db").let { db -> db.isFile && db.canRead() } }
            .toList()
    }

    /** True if [name] is an Android-internal mount we should skip. */
    private fun isInternalStorage(name: String): Boolean = when (name) {
        "emulated", "self", "enc_emulated" -> true
        else -> false
    }
}
