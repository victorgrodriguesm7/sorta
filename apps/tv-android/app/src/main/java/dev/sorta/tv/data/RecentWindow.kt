package dev.sorta.tv.data

import java.time.Instant
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit

/**
 * "Recently added" cutoff calculator. ISO 8601 UTC second-precision
 * is the shape `media.catalogued_at` uses (see desktop migration
 * 0004), and string comparison sorts chronologically — so we just
 * compare strings in the SQL query.
 *
 * Extracted from [dev.sorta.tv.ui.BrowseFragment] so the window
 * arithmetic is unit-testable without an Android runtime.
 */
object RecentWindow {

    /** Default "recently added" window in days. */
    const val DEFAULT_DAYS: Long = 14

    /**
     * Build the cutoff timestamp: `now − [days]`, formatted as ISO 8601
     * UTC at second precision (`YYYY-MM-DDTHH:MM:SSZ`). Always 20 chars.
     */
    fun cutoff(days: Long = DEFAULT_DAYS, now: Instant = Instant.now()): String {
        val cutoff = now.minus(days, ChronoUnit.DAYS).truncatedTo(ChronoUnit.SECONDS)
        return DateTimeFormatter.ISO_INSTANT.format(cutoff)
    }
}
