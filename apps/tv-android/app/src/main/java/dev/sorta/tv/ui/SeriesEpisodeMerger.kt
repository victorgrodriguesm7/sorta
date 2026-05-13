package dev.sorta.tv.ui

import dev.sorta.tv.data.Episode
import dev.sorta.tv.data.EpisodeRow
import dev.sorta.tv.data.Season
import java.io.File

/**
 * Unified episode-list model used by the redesigned series screen.
 * Built from two sources merged in priority order:
 *
 *   1. `episodes` table rows from `MediaRepository.listEpisodes`.
 *      These carry real TMDB metadata (title, overview, still) so
 *      they're preferred whenever they exist.
 *   2. Filesystem walk results from `SeriesScanner`. Used as a
 *      fallback for v3 drives, or to surface files the user has
 *      on disk but hasn't run Re-Catalog over yet.
 *
 * Both sources can be partial; the merger handles "table only",
 * "disk only", and the mixed case (e.g. table has S01E01 but disk
 * has S01E01 + S01E02 — keep the rich row for S01E01 and append a
 * synthesized row for S01E02).
 *
 * Pure JVM — no Android deps in the merger so the layered behaviour
 * stays unit-testable.
 */
data class SeriesEpisodeItem(
    val seasonNumber: Int,
    val episodeNumber: Int,
    /** Display title — TMDB title when available, SxxExx label otherwise. */
    val title: String,
    /** Full TMDB overview text; renderer trims for display. */
    val overview: String?,
    val airDate: String?,
    val runtimeMinutes: Int?,
    /** Per-episode TMDB still, relative to HD root. Null on v3 / no fetch. */
    val stillPath: String?,
    /** TMDB CDN fallback. */
    val stillUrl: String?,
    /** Resolved file on disk, when known. May be null on table rows whose
     *  `file_path` couldn't be resolved (e.g. moved manually). */
    val file: File?,
)

data class SeriesEpisodeSection(
    val seasonNumber: Int,
    val items: List<SeriesEpisodeItem>,
)

object SeriesEpisodeMerger {

    /**
     * Combine [table] + [disk] into the renderer's view model.
     * `driveRoot` is only needed when a table row's `file_path` is
     * relative — pass null to leave [SeriesEpisodeItem.file] resolved
     * to the raw string when present, or null when absent.
     */
    fun merge(
        table: List<EpisodeRow>,
        disk: List<Season>,
        driveRoot: File? = null,
    ): List<SeriesEpisodeSection> {
        // Index disk-side files by (season, episode) for quick lookup.
        val diskByKey: Map<Pair<Int, Int>, Episode> = disk
            .asSequence()
            .flatMap { season ->
                season.episodes.asSequence().mapNotNull { ep ->
                    val s = ep.seasonNumber ?: season.number ?: return@mapNotNull null
                    val e = ep.episodeNumber ?: return@mapNotNull null
                    (s to e) to ep
                }
            }
            .toMap()

        val tableItems = table.map { row ->
            val key = row.seasonNumber to row.episodeNumber
            val diskMatch = diskByKey[key]
            SeriesEpisodeItem(
                seasonNumber = row.seasonNumber,
                episodeNumber = row.episodeNumber,
                title = row.title?.takeIf { it.isNotBlank() }
                    ?: "S%02dE%02d".format(row.seasonNumber, row.episodeNumber),
                overview = row.overview,
                airDate = row.airDate,
                runtimeMinutes = row.runtimeMinutes,
                stillPath = row.stillPath,
                stillUrl = row.stillUrl,
                file = diskMatch?.file
                    ?: row.filePath?.let { rel ->
                        if (driveRoot != null) File(driveRoot, rel) else File(rel)
                    },
            )
        }

        // Keys already covered by the table — disk-only fallback adds
        // the rest.
        val coveredKeys = tableItems.map { it.seasonNumber to it.episodeNumber }.toSet()
        val diskOnlyItems = diskByKey
            .filterKeys { it !in coveredKeys }
            .map { (key, ep) ->
                val (s, e) = key
                SeriesEpisodeItem(
                    seasonNumber = s,
                    episodeNumber = e,
                    title = ep.label,
                    overview = null,
                    airDate = null,
                    runtimeMinutes = null,
                    stillPath = null,
                    stillUrl = null,
                    file = ep.file,
                )
            }

        val all = (tableItems + diskOnlyItems)
            .sortedWith(compareBy({ it.seasonNumber }, { it.episodeNumber }))

        return all.groupBy { it.seasonNumber }
            .map { (season, items) -> SeriesEpisodeSection(season, items) }
            .sortedBy { it.seasonNumber }
    }

    /**
     * Trim an overview to at most [max] characters for the
     * compact list item, appending an ellipsis when truncation
     * happened. Null in → null out. The plan calls for 200 chars
     * — `max` is a parameter only so the test can vary it.
     */
    fun snippet(text: String?, max: Int = 200): String? {
        if (text == null) return null
        return if (text.length <= max) text else text.take(max) + "…"
    }
}
