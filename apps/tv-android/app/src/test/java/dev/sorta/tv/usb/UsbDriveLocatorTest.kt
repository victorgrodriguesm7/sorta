package dev.sorta.tv.usb

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class UsbDriveLocatorTest {

    @get:Rule val tmp = TemporaryFolder()

    @Test
    fun locate_findsDirectoriesContainingSortaDb() {
        val storage = tmp.newFolder("storage")
        val usb1 = File(storage, "usb6601").apply { mkdirs() }
        File(usb1, "sorta.db").writeText("")
        // Distractor: a child without sorta.db.
        File(storage, "usbZZZZ").mkdirs()

        val drives = UsbDriveLocator.locate(storage)

        assertEquals(1, drives.size)
        assertEquals(usb1.absolutePath, drives.single().absolutePath)
    }

    @Test
    fun locate_skipsInternalStorageMounts() {
        val storage = tmp.newFolder("storage")
        val emulated = File(storage, "emulated").apply { mkdirs() }
        // Even if internal storage somehow has a sorta.db, ignore it —
        // we only want removable media.
        File(emulated, "sorta.db").writeText("")
        File(storage, "self").mkdirs()

        assertTrue(UsbDriveLocator.locate(storage).isEmpty())
    }

    @Test
    fun locate_returnsAllMatchingDrivesIfMultiple() {
        val storage = tmp.newFolder("storage")
        val usb1 = File(storage, "usb1").apply { mkdirs() }
        File(usb1, "sorta.db").writeText("")
        val usb2 = File(storage, "udisk0").apply { mkdirs() }
        File(usb2, "sorta.db").writeText("")

        val drives = UsbDriveLocator.locate(storage).map { it.name }.toSet()

        assertEquals(setOf("usb1", "udisk0"), drives)
    }

    @Test
    fun locate_returnsEmptyWhenStorageRootMissing() {
        val missing = File(tmp.root, "does-not-exist")
        assertTrue(UsbDriveLocator.locate(missing).isEmpty())
    }

    @Test
    fun locate_skipsFilesAtTopLevel() {
        val storage = tmp.newFolder("storage")
        // A regular file pretending to be a mount — should be ignored.
        File(storage, "usb1").writeText("not a directory")

        assertTrue(UsbDriveLocator.locate(storage).isEmpty())
    }
}
