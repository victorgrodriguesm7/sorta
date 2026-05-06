package dev.sorta.tv.data

import dev.sorta.tv.data.SchemaCompat.shouldRefuse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SchemaCompatTest {

    @Test
    fun matchingVersionsAreOk() {
        assertEquals(SchemaCompat.Result.Ok, SchemaCompat.isCompatible(3, 3))
    }

    @Test
    fun olderOnDiskIsTolerated() {
        val result = SchemaCompat.isCompatible(onDisk = 2, known = 3)
        assertEquals(SchemaCompat.Result.OnDiskOlderTolerated(2, 3), result)
        assertFalse(result.shouldRefuse())
    }

    @Test
    fun newerOnDiskIsRefused() {
        val result = SchemaCompat.isCompatible(onDisk = 4, known = 3)
        assertEquals(SchemaCompat.Result.OnDiskNewer(4, 3), result)
        assertTrue(result.shouldRefuse())
    }

    @Test
    fun defaultsToCompiledKnownVersion() {
        // Sanity check: the default known parameter is the build constant.
        val result = SchemaCompat.isCompatible(SchemaCompat.KNOWN_SCHEMA_VERSION)
        assertEquals(SchemaCompat.Result.Ok, result)
    }
}
