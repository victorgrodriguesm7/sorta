package dev.sorta.tv.playback

import dev.sorta.tv.data.DiskFormat
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import java.io.File

/**
 * Picks the concrete video file to launch playback for, given a
 * catalogued [MediaRow] and the drive root.
 *
 * For movies: there's at most one playable file directly inside
 * `folder_path` (per docs/disk-format.md). For series: walk into the
 * first `Season N/` subfolder and pick the lexicographically first
 * episode — Phase 4 doesn't ship a season picker yet, so first
 * episode is the sensible default.
 */
object PlaybackResolver {

    fun resolve(driveRoot: File, media: MediaRow): File? {
        val mediaFolder = File(driveRoot, media.folderPath)
        if (!mediaFolder.isDirectory) return null
        return when (media.mediaType) {
            MediaType.MOVIE -> firstPlayableVideo(mediaFolder)
            MediaType.TV -> firstPlayableEpisode(mediaFolder)
        }
    }

    /** First non-hidden video file directly inside [dir]. */
    private fun firstPlayableVideo(dir: File): File? {
        return dir.listFiles()
            ?.asSequence()
            ?.filter { it.isFile }
            ?.filter { !DiskFormat.isHiddenForReader(it.name) }
            ?.filter { DiskFormat.isVideoFile(it.name) }
            ?.sortedBy { it.name }
            ?.firstOrNull()
    }

    /**
     * For a series root, walk the season subfolders in order and
     * return the first playable episode.
     */
    private fun firstPlayableEpisode(seriesDir: File): File? {
        val seasons = seriesDir.listFiles()
            ?.asSequence()
            ?.filter { it.isDirectory }
            ?.filter { !DiskFormat.isHiddenForReader(it.name) }
            ?.sortedBy { it.name }
            ?.toList()
            .orEmpty()
        for (season in seasons) {
            firstPlayableVideo(season)?.let { return it }
        }
        return null
    }
}
