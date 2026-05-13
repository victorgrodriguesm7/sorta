package dev.sorta.tv.data

/**
 * The set of languages the user can pick from in Settings. Backed
 * by a small enum (rather than a free-form BCP 47 string) so we
 * never have to deal with "did they type 'pt-BR' or 'pt_BR' or
 * 'pt-br'?" — `fromTag` normalises on read.
 *
 * Persisted as the [tag] string under `SharedPreferences` key
 * `ui_language`.
 */
enum class LocaleChoice(val tag: String) {
    /** Follow the device's primary system locale. Default. */
    SYSTEM("system"),
    EN("en"),
    PT_BR("pt-BR");

    companion object {
        /**
         * Parse a persisted tag back to its enum. Null, empty,
         * `"system"`, or an unknown value all collapse to
         * [SYSTEM] — that keeps the user out of a soft-broken
         * state if a later build wrote a tag this build doesn't
         * recognise yet.
         */
        fun fromTag(raw: String?): LocaleChoice {
            if (raw.isNullOrEmpty()) return SYSTEM
            // BCP-47 hyphen is canonical, but tolerate an
            // underscore in case an older API stored it that way.
            val normalised = raw.replace('_', '-')
            return entries.firstOrNull { it.tag.equals(normalised, ignoreCase = false) }
                ?: SYSTEM
        }
    }
}
