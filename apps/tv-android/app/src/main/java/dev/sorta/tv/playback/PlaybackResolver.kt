package dev.sorta.tv.playback

import android.util.Log
import dev.sorta.tv.data.DiskFormat
import dev.sorta.tv.data.MediaRow
import dev.sorta.tv.data.MediaType
import dev.sorta.tv.data.TmdbTagParser
import java.io.File

/**
 * Picks the concrete video file to launch playback for, given a
 * catalogued [MediaRow] and the drive root.
 *
 * Resolution order:
 *
 *   1. Exact path: `<driveRoot>/<media.folder_path>`. This is what the
 *      desktop wrote and what we should hit on a clean drive.
 *
 *   2. Tmdb-id fallback: walk under `<driveRoot>` for any directory
 *      whose name ends in `[tmdb-{media.tmdbId}]`. Catches the cases
 *      where the on-disk path drifted from the catalog — different
 *      Unicode normalization on USB-mounted exFAT (`Ação` NFC vs NFD),
 *      a manual rename, or case-folding quirks on FAT filesystems.
 *
 * For movies: there's at most one playable file directly inside the
 * folder. For series: walk into the lexicographically first season
 * subfolder and pick the first episode — Phase 4 doesn't ship a
 * season picker yet, so first episode is the sensible default.
 */
object PlaybackResolver {

    private const val TAG = "PlaybackResolver"

    fun resolve(driveRoot: File, media: MediaRow): File? {
        val folder = locateMediaFolder(driveRoot, media) ?: return null
        return when (media.mediaType) {
            MediaType.MOVIE -> firstPlayableVideo(folder)
            MediaType.TV -> firstPlayableEpisode(folder)
        }
    }

    private fun locateMediaFolder(driveRoot: File, media: MediaRow): File? {
        val exact = File(driveRoot, media.folderPath)
        if (exact.isDirectory) return exact

        // Fallback: hunt for a directory tagged with the same TMDB id.
        // Bounded in depth (4 levels) to keep this O(catalog size).
        Log.w(
            TAG,
            "exact path missing for tmdb-${media.tmdbId} (${media.folderPath}); scanning",
        )
        val match = findByTmdbId(driveRoot, media.tmdbId, depth = 4)
        if (match != null) {
            Log.w(TAG, "tmdb-${media.tmdbId} resolved to ${match.absolutePath}")
        } else {
            Log.w(TAG, "tmdb-${media.tmdbId} not found anywhere under $driveRoot")
        }
        return match
    }

    private fun findByTmdbId(root: File, tmdbId: Long, depth: Int): File? {
        if (depth < 0) return null
        val children = root.listFiles() ?: return null
        for (child in children) {
            if (!child.isDirectory) continue
            if (DiskFormat.isHiddenForReader(child.name)) continue
            if (TmdbTagParser.parseTmdbId(child.name) == tmdbId) return child
            findByTmdbId(child, tmdbId, depth - 1)?.let { return it }
        }
        return null
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
