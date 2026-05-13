package dev.sorta.tv.playback

import org.junit.Assert.assertEquals
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
    fun build_carriesAbsoluteFilePath() {
        val path = "/storage/usb1/Movies/Action/Inception [tmdb-27205]/Inception [tmdb-27205].mkv"
        val req = PlaybackIntent.build(File(path))

        // We hand the raw path to the UI layer so it can build the
        // triple-slash file:/// URI via Uri.fromFile — see the
        // PlaybackRequest.filePath kdoc for why.
        // Normalize separators — on a Windows test host `File("/…").path`
        // comes back with backslashes; the production target (Android)
        // keeps the forward slashes from the catalog.
        assertEquals(path, req.filePath.replace('\\', '/'))
    }

    @Test
    fun build_includesGrantReadFlag() {
        val req = PlaybackIntent.build(File("/x.mkv"))

        val flag = PlaybackRequest.FLAG_GRANT_READ_URI_PERMISSION
        assertEquals(flag, req.flags and flag)
    }
}
