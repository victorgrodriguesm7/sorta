package dev.sorta.tv.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

class ManifestTest {

    @Test
    fun fromJson_parsesCanonicalShape() {
        val text = """
            {
              "schema_version": 3,
              "app_version": "0.1.0",
              "generated_at": "2026-05-15T12:34:56Z",
              "counts": { "media_total": 142, "movies": 98, "series": 44 }
            }
        """.trimIndent()

        val m = Manifest.fromJson(text)

        assertEquals(3, m.schemaVersion)
        assertEquals("0.1.0", m.appVersion)
        assertEquals("2026-05-15T12:34:56Z", m.generatedAt)
        assertEquals(142, m.counts.mediaTotal)
        assertEquals(98, m.counts.movies)
        assertEquals(44, m.counts.series)
    }

    @Test
    fun fromJson_toleratesUnknownFields() {
        val text = """
            {
              "schema_version": 3,
              "future_thing": { "weight": 9.9 },
              "extra_top_level": "ignored",
              "counts": { "media_total": 1, "movies": 1, "series": 0, "future_count": 7 }
            }
        """.trimIndent()

        val m = Manifest.fromJson(text)

        assertEquals(3, m.schemaVersion)
        assertEquals(1, m.counts.mediaTotal)
    }

    @Test
    fun fromJson_defaultsMissingOptionalFields() {
        val text = """{ "schema_version": 1 }"""

        val m = Manifest.fromJson(text)

        assertEquals(1, m.schemaVersion)
        assertNull(m.appVersion)
        assertNull(m.generatedAt)
        assertEquals(0, m.counts.mediaTotal)
        assertEquals(0, m.counts.movies)
        assertEquals(0, m.counts.series)
    }

    @Test
    fun fromJson_rejectsMalformedJson() {
        assertThrows(IllegalArgumentException::class.java) {
            Manifest.fromJson("not json at all")
        }
        assertThrows(IllegalArgumentException::class.java) {
            Manifest.fromJson("[1,2,3]")
        }
    }

    @Test
    fun fromJson_rejectsMissingSchemaVersion() {
        assertThrows(IllegalArgumentException::class.java) {
            Manifest.fromJson("""{ "app_version": "0.1.0" }""")
        }
    }

    @Test
    fun fromJson_rejectsNegativeSchemaVersion() {
        assertThrows(IllegalArgumentException::class.java) {
            Manifest.fromJson("""{ "schema_version": -1 }""")
        }
    }

    @Test
    fun fromJson_rejectsNonIntegerSchemaVersion() {
        assertThrows(IllegalArgumentException::class.java) {
            Manifest.fromJson("""{ "schema_version": "three" }""")
        }
    }
}
