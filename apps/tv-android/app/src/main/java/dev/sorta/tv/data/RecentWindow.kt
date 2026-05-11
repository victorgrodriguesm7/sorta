package dev.sorta.tv.data

import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/**
 * "Recently added" cutoff calculator. ISO 8601 UTC second-precision
 * is the shape `media.catalogued_at` uses (see desktop migration
 * 0004), and string comparison sorts chronologically — so we just
 * compare strings in the SQL query.
 *
 * Uses `Date` + `SimpleDateFormat` rather than `java.time.Instant`
 * so the code runs on API 21 without core-library desugaring (a
 * `NoClassDefFoundError: java.time.Instant` on older TVs would be
 * a hard crash on the browse screen).
 *
 * Extracted from [dev.sorta.tv.ui.BrowseFragment] so the window
 * arithmetic is unit-testable without an Android runtime.
 */
object RecentWindow {

    /** Default "recently added" window in days. */
    const val DEFAULT_DAYS: Long = 14

    private const val MILLIS_PER_DAY: Long = 24L * 60L * 60L * 1000L

    /**
     * Build the cutoff timestamp: `now − [days]`, formatted as ISO 8601
     * UTC at second precision (`YYYY-MM-DDTHH:MM:SSZ`). Always 20 chars.
     *
     * [nowMillis] is unix epoch millis; the default is wall-clock time.
     * Tests pin this to a fixed value for determinism.
     */
    fun cutoff(days: Long = DEFAULT_DAYS, nowMillis: Long = System.currentTimeMillis()): String {
        val cutoffMillis = nowMillis - days * MILLIS_PER_DAY
        // SimpleDateFormat is not thread-safe; build a fresh one each
        // call. This is invoked once per browse refresh so the cost
        // is negligible.
        val fmt = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.US).apply {
            timeZone = TimeZone.getTimeZone("UTC")
        }
        return fmt.format(Date(cutoffMillis))
    }
}
