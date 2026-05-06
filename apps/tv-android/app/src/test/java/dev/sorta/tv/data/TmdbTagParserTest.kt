package dev.sorta.tv.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TmdbTagParserTest {

    @Test
    fun parseTmdbId_extractsWhenPresent() {
        assertEquals(27205L, TmdbTagParser.parseTmdbId("Inception [tmdb-27205]"))
        assertEquals(1L, TmdbTagParser.parseTmdbId("Some Title [tmdb-1]"))
    }

    @Test
    fun parseTmdbId_toleratesTrailingWhitespace() {
        assertEquals(42L, TmdbTagParser.parseTmdbId("Foo [tmdb-42]   "))
    }

    @Test
    fun parseTmdbId_returnsNullForUncatalogued() {
        assertNull(TmdbTagParser.parseTmdbId("Inception"))
        assertNull(TmdbTagParser.parseTmdbId("Inception (2010)"))
        assertNull(TmdbTagParser.parseTmdbId("[tmdb-]"))
        assertNull(TmdbTagParser.parseTmdbId(""))
    }

    @Test
    fun stripTmdbTag_returnsBareTitle() {
        assertEquals("Inception", TmdbTagParser.stripTmdbTag("Inception [tmdb-27205]"))
        assertEquals("Cidade de Deus", TmdbTagParser.stripTmdbTag("Cidade de Deus [tmdb-598]   "))
    }

    @Test
    fun stripTmdbTag_returnsNullWhenTagAbsentOrTitleEmpty() {
        assertNull(TmdbTagParser.stripTmdbTag("Inception"))
        assertNull(TmdbTagParser.stripTmdbTag("[tmdb-1]"))
        assertNull(TmdbTagParser.stripTmdbTag(""))
    }

    @Test
    fun isCataloguedFolder_requiresTitlePrefix() {
        assertTrue(TmdbTagParser.isCataloguedFolder("Inception [tmdb-27205]"))
        assertTrue(TmdbTagParser.isCataloguedFolder("Foo [tmdb-1]   "))
        assertFalse(TmdbTagParser.isCataloguedFolder("[tmdb-1]"))
        assertFalse(TmdbTagParser.isCataloguedFolder("Inception"))
        assertFalse(TmdbTagParser.isCataloguedFolder(""))
    }
}
