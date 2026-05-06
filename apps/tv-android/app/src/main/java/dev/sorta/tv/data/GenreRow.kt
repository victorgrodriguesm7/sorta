package dev.sorta.tv.data

/**
 * One row from the `genres` table. The on-disk folder name (and the
 * label we render in the UI) is [displayName] — the user's translation
 * if set, otherwise the canonical English name from TMDB.
 */
data class GenreRow(
    val id: Long,
    val mediaType: MediaType,
    val canonicalName: String,
    val translatedName: String?,
) {
    val displayName: String get() = translatedName ?: canonicalName
}
