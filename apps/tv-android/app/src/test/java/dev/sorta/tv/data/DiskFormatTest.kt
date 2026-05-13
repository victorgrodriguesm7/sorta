package dev.sorta.tv.data

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DiskFormatTest {

    @Test
    fun isVideoFile_acceptsKnownExtensions() {
        listOf(
            "Inception.mkv",
            "movie.MP4",
            "Old.avi",
            "phone.mov",
            "ancient.WMV",
            "thing.m4v",
            "stream.webm",
        ).forEach { assertTrue(it, DiskFormat.isVideoFile(it)) }
    }

    @Test
    fun isVideoFile_rejectsNonVideoOrMissingExtension() {
        assertFalse(DiskFormat.isVideoFile("notes.txt"))
        assertFalse(DiskFormat.isVideoFile("subtitles.srt"))
        assertFalse(DiskFormat.isVideoFile("metadata.nfo"))
        assertFalse(DiskFormat.isVideoFile("README"))
        assertFalse(DiskFormat.isVideoFile(""))
        assertFalse(DiskFormat.isVideoFile("trailing."))
    }

    @Test
    fun isHiddenForReader_hidesSortaInternalMarkers() {
        assertTrue(DiskFormat.isHiddenForReader("Inception [tmdb-1].original.mkv"))
        assertTrue(DiskFormat.isHiddenForReader("Inception [tmdb-1].compressing.mkv"))
    }

    @Test
    fun isHiddenForReader_hidesPosterCache() {
        assertTrue(DiskFormat.isHiddenForReader("poster"))
    }

    @Test
    fun isHiddenForReader_hidesOsJunk() {
        assertTrue(DiskFormat.isHiddenForReader("\$RECYCLE.BIN"))
        assertTrue(DiskFormat.isHiddenForReader("\$Anything"))
        assertTrue(DiskFormat.isHiddenForReader("System Volume Information"))
        assertTrue(DiskFormat.isHiddenForReader(".Trash"))
        assertTrue(DiskFormat.isHiddenForReader(".Trashes"))
        assertTrue(DiskFormat.isHiddenForReader(".Spotlight-V100"))
        assertTrue(DiskFormat.isHiddenForReader(".fseventsd"))
        assertTrue(DiskFormat.isHiddenForReader("lost+found"))
    }

    @Test
    fun isHiddenForReader_keepsRegularNames() {
        assertFalse(DiskFormat.isHiddenForReader("Movies"))
        assertFalse(DiskFormat.isHiddenForReader("Series"))
        assertFalse(DiskFormat.isHiddenForReader("Inception [tmdb-27205]"))
        assertFalse(DiskFormat.isHiddenForReader("Inception [tmdb-27205].mkv"))
        assertFalse(DiskFormat.isHiddenForReader(""))
    }
}
