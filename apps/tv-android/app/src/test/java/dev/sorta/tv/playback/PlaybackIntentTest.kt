package dev.sorta.tv.playback

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class PlaybackIntentTest {

    @Test
    fun build_setsViewActionAndVideoMime() {
        val req = PlaybackIntent.build(File("/storage/usb1/Movies/Action/Mad Max [tmdb-76341]/Mad Max [tmdb-76341].mkv"))

        assertEquals(PlaybackRequest.ACTION_VIEW, req.action)
        assertEquals("video/*", req.mimeType)
    }

    @Test
    fun build_emitsFileSchemeUri() {
        val req = PlaybackIntent.build(File("/storage/usb1/Movies/Action/Inception [tmdb-27205]/Inception [tmdb-27205].mkv"))

        assertTrue("expected file:// URI, got: ${req.uri}", req.uri.startsWith("file:/"))
        // The path must round-trip through the URI — spaces become %20.
        assertTrue(req.uri.endsWith("Inception%20%5Btmdb-27205%5D.mkv"))
    }

    @Test
    fun build_includesGrantReadFlag() {
        val req = PlaybackIntent.build(File("/x.mkv"))

        val flag = PlaybackRequest.FLAG_GRANT_READ_URI_PERMISSION
        assertEquals(flag, req.flags and flag)
    }
}
