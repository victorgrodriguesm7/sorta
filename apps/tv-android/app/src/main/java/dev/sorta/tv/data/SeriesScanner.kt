package dev.sorta.tv.data

import java.io.File

/**
 * Walks a series folder produced by the desktop and returns its
 * seasons + episodes as a typed structure the UI can render.
 *
 * Layout (from `docs/disk-format.md`):
 *
 *   <series root>/
 *     <Season label> 1/
 *       S01E01.mkv
 *       S01E02.mkv
 *     <Season label> 2/
 *       …
 *
 * The `<Season label>` prefix is user-translatable (defaults to
 * "Season"), so we don't anchor on it. Instead we treat every
 * non-hidden subdirectory as a season and pull the season number out
 * of the directory name with a digits-only regex; if no digits match,
 * the season number is null and the row is sorted by name.
 *
 * Inside each season folder, video files are matched by extension
 * via [DiskFormat.isVideoFile] and filtered against
 * [DiskFormat.isHiddenForReader]. If filenames follow the
 * `S{XX}E{YY}` convention (Sorta's default rename pattern) we extract
 * the season+episode numbers; otherwise the episode keeps its raw
 * basename and uses the file's lexical order.
 *
 * Pure JVM — takes a [File] in, returns plain data, no Android deps.
 */
data class Episode(
    val file: File,
    /** Display label, e.g. "S01E03" or the bare filename if unparsed. */
    val label: String,
    /** Parsed season number from filename, if `S{XX}E{YY}` matched. */
    val seasonNumber: Int?,
    /** Parsed episode number from filename, if `S{XX}E{YY}` matched. */
    val episodeNumber: Int?,
)

data class Season(
    /** Display label — the on-disk folder name. */
    val label: String,
    /** Parsed season number from folder name, if any digit run found. */
    val number: Int?,
    val episodes: List<Episode>,
)

object SeriesScanner {

    private val EPISODE_TAG = Regex("""(?i)S(\d{1,3})E(\d{1,3})""")
    private val LEADING_DIGITS = Regex("""\d+""")

    /**
     * @param seriesRoot the directory `<driveRoot>/<series.folder_path>`
     * @return seasons in display order — by parsed number when
     *         present, falling back to alphabetic on the folder name.
     *         Empty seasons (no playable video files) are dropped.
     */
    fun scan(seriesRoot: File): List<Season> {
        if (!seriesRoot.isDirectory) return emptyList()
        val seasonDirs = seriesRoot.listFiles()
            ?.asSequence()
            ?.filter { it.isDirectory }
            ?.filter { !DiskFormat.isHiddenForReader(it.name) }
            ?.toList()
            .orEmpty()

        return seasonDirs
            .map(::buildSeason)
            .filter { it.episodes.isNotEmpty() }
            .sortedWith(
                compareBy(
                    { it.number ?: Int.MAX_VALUE },
                    { it.label.lowercase() },
                ),
            )
    }

    private fun buildSeason(dir: File): Season {
        val files = dir.listFiles()
            ?.asSequence()
            ?.filter { it.isFile }
            ?.filter { !DiskFormat.isHiddenForReader(it.name) }
            ?.filter { DiskFormat.isVideoFile(it.name) }
            ?.toList()
            .orEmpty()
        val episodes = files.map(::buildEpisode).sortedWith(
            compareBy(
                { it.episodeNumber ?: Int.MAX_VALUE },
                { it.label.lowercase() },
            ),
        )
        return Season(
            label = dir.name,
            number = LEADING_DIGITS.find(dir.name)?.value?.toIntOrNull(),
            episodes = episodes,
        )
    }

    private fun buildEpisode(file: File): Episode {
        val basename = file.nameWithoutExtension
        val match = EPISODE_TAG.find(basename)
        return if (match != null) {
            Episode(
                file = file,
                label = "S${pad2(match.groupValues[1])}E${pad2(match.groupValues[2])}",
                seasonNumber = match.groupValues[1].toIntOrNull(),
                episodeNumber = match.groupValues[2].toIntOrNull(),
            )
        } else {
            Episode(
                file = file,
                label = basename,
                seasonNumber = null,
                episodeNumber = null,
            )
        }
    }

    private fun pad2(s: String): String = s.padStart(2, '0')
}
