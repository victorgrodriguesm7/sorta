package dev.sorta.tv.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * Unit tests for the multi-drive [CatalogCheck]. The locator is
 * exercised against the temp-folder storage root, and the
 * `schema_version` reader is faked so we don't need Android SQLite
 * on the JVM.
 */
class CatalogCheckTest {

    @get:Rule val tmp = TemporaryFolder()

    /** Create `<storage>/<name>/sorta.db` so the locator picks it up. */
    private fun makeDrive(name: String): File {
        val drive = File(tmp.root, name).apply { mkdirs() }
        File(drive, "sorta.db").writeText("stub")
        return drive
    }

    @Test
    fun noDrivesReturnsNoDrive() {
        val result = CatalogCheck.run(storageRoot = tmp.root) { error("never opened") }
        assertEquals(CatalogCheck.Result.NoDrive, result)
    }

    @Test
    fun singleCompatibleDriveReturnsOk() {
        val a = makeDrive("usbA")
        val result = CatalogCheck.run(storageRoot = tmp.root) { 4 }
        val ok = result as CatalogCheck.Result.Ok
        assertEquals(listOf(a), ok.driveRoots)
        assertEquals(4, ok.schemaVersion)
    }

    @Test
    fun threeDrivesAtSameVersionAllLoad() {
        val a = makeDrive("usbA")
        val b = makeDrive("usbB")
        val c = makeDrive("usbC")
        val result = CatalogCheck.run(storageRoot = tmp.root) { 4 }
        val ok = result as CatalogCheck.Result.Ok
        // Compare as sets so we don't depend on listFiles() order.
        assertEquals(setOf(a, b, c), ok.driveRoots.toSet())
        assertEquals(4, ok.schemaVersion)
    }

    @Test
    fun mixedVersionsLoadOnlyTheHighest() {
        // Spec: V3, V3, V5 → load only V5.
        val v3a = makeDrive("usb3A")
        val v3b = makeDrive("usb3B")
        val v5 = makeDrive("usb5")
        val versions = mapOf(v3a to 3, v3b to 3, v5 to 5)
        val result = CatalogCheck.run(
            storageRoot = tmp.root,
            known = 5,
        ) { dbFile -> versions[dbFile.parentFile!!] }
        val ok = result as CatalogCheck.Result.Ok
        assertEquals(listOf(v5), ok.driveRoots)
        assertEquals(5, ok.schemaVersion)
    }

    @Test
    fun highestVersionNewerThanKnownRefuses() {
        makeDrive("usbOld")
        makeDrive("usbNew")
        val versions = mapOf("usbOld" to 4, "usbNew" to 7)
        val result = CatalogCheck.run(
            storageRoot = tmp.root,
            known = 4,
        ) { dbFile -> versions[dbFile.parentFile!!.name] }
        assertEquals(CatalogCheck.Result.SchemaTooNew(onDisk = 7, known = 4), result)
    }

    @Test
    fun driveMissingSchemaVersionSettingIsTreatedAsZero() {
        // A blank DB with no `settings` rows is the v3 fallback path —
        // `setting("schema_version")` returns null, which we treat as
        // version 0 for ordering. A null result alongside a real
        // versioned drive must lose to the real drive.
        val zero = makeDrive("usbZero")
        val current = makeDrive("usbV4")
        val perDrive = mapOf<File, Int?>(zero to null, current to 4)
        val result = CatalogCheck.run(
            storageRoot = tmp.root,
            known = 4,
        ) { dbFile -> perDrive[dbFile.parentFile!!] }
        val ok = result as CatalogCheck.Result.Ok
        assertEquals(listOf(current), ok.driveRoots)
        assertEquals(4, ok.schemaVersion)
    }

    @Test
    fun allDrivesAtSameOlderVersionLoadAtThatVersion() {
        // Three v3 drives, reader build knows v4. All load — they're
        // the highest version available even though the reader knows
        // a newer one. Matches the existing OnDiskOlderTolerated path.
        val a = makeDrive("usb3A")
        val b = makeDrive("usb3B")
        val result = CatalogCheck.run(
            storageRoot = tmp.root,
            known = 4,
        ) { 3 }
        val ok = result as CatalogCheck.Result.Ok
        assertEquals(setOf(a, b), ok.driveRoots.toSet())
        assertEquals(3, ok.schemaVersion)
    }
}
