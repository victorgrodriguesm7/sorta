package dev.sorta.tv.data

import android.content.Context
import android.content.SharedPreferences

/**
 * Thin SharedPreferences wrapper for the user's language choice.
 * The parsing/normalisation lives in [LocaleChoice]; this class is
 * just persistence.
 *
 * Keyed under [PREFS] so it's independent of any future global
 * "settings" pref file we might want to add.
 */
class LocalePreference(context: Context) {
    private val prefs: SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun get(): LocaleChoice = LocaleChoice.fromTag(prefs.getString(KEY, null))

    fun set(choice: LocaleChoice) {
        prefs.edit().putString(KEY, choice.tag).apply()
    }

    companion object {
        private const val PREFS = "ui_prefs"
        private const val KEY = "ui_language"
    }
}
