package dev.sorta.tv.data

import dev.sorta.tv.data.SchemaCompat.shouldRefuse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SchemaCompatTest {

    @Test
    fun matchingVersionsAreOk() {
        assertEquals(SchemaCompat.Result.Ok, SchemaCompat.isCompatible(4, 4))
    }

    @Test
    fun olderOnDiskIsTolerated() {
        // A v3 drive (no episodes table, no is_new column) is still
        // openable; the reader degrades the Recently Added row + the
        // per-episode view but every other surface keeps working.
        val result = SchemaCompat.isCompatible(onDisk = 3, known = 4)
        assertEquals(SchemaCompat.Result.OnDiskOlderTolerated(3, 4), result)
        assertFalse(result.shouldRefuse())
    }

    @Test
    fun newerOnDiskIsRefused() {
        val result = SchemaCompat.isCompatible(onDisk = 5, known = 4)
        assertEquals(SchemaCompat.Result.OnDiskNewer(5, 4), result)
        assertTrue(result.shouldRefuse())
    }

    @Test
    fun knownVersionMatchesDesktopSchemaV4() {
        // Locked in alongside the desktop's CURRENT_SCHEMA_VERSION = 4
        // and migration 0004_episodes_and_flags.sql. Bump in sync.
        assertEquals(4, SchemaCompat.KNOWN_SCHEMA_VERSION)
    }

    @Test
    fun defaultsToCompiledKnownVersion() {
        // Sanity check: the default known parameter is the build constant.
        val result = SchemaCompat.isCompatible(SchemaCompat.KNOWN_SCHEMA_VERSION)
        assertEquals(SchemaCompat.Result.Ok, result)
    }
}
