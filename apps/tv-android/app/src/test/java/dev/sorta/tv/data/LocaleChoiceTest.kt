package dev.sorta.tv.data

import org.junit.Assert.assertEquals
import org.junit.Test

class LocaleChoiceTest {

    @Test
    fun systemIsTheDefaultWhenNothingPersisted() {
        // A user who has never opened the language picker is on
        // "follow the system" — the cleanest fallback for first run.
        assertEquals(LocaleChoice.SYSTEM, LocaleChoice.fromTag(null))
        assertEquals(LocaleChoice.SYSTEM, LocaleChoice.fromTag(""))
    }

    @Test
    fun systemTagRoundTrips() {
        assertEquals(LocaleChoice.SYSTEM, LocaleChoice.fromTag("system"))
        assertEquals("system", LocaleChoice.SYSTEM.tag)
    }

    @Test
    fun englishTagRoundTrips() {
        assertEquals(LocaleChoice.EN, LocaleChoice.fromTag("en"))
        assertEquals("en", LocaleChoice.EN.tag)
    }

    @Test
    fun portugueseBrazilTagRoundTrips() {
        // BCP 47 uses a hyphen, but Android's older parsers tolerate
        // an underscore. Both forms must map to the same enum.
        assertEquals(LocaleChoice.PT_BR, LocaleChoice.fromTag("pt-BR"))
        assertEquals(LocaleChoice.PT_BR, LocaleChoice.fromTag("pt_BR"))
        assertEquals("pt-BR", LocaleChoice.PT_BR.tag)
    }

    @Test
    fun unknownTagFallsBackToSystem() {
        // Forward-compat: a tag written by a later build that we
        // don't know about (e.g. "es-419") must not crash the app.
        assertEquals(LocaleChoice.SYSTEM, LocaleChoice.fromTag("es-419"))
        assertEquals(LocaleChoice.SYSTEM, LocaleChoice.fromTag("garbage"))
    }
}
