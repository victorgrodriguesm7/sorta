package dev.sorta.tv.ui

import android.os.Bundle
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import androidx.fragment.app.FragmentActivity
import androidx.leanback.app.GuidedStepSupportFragment
import androidx.leanback.widget.GuidanceStylist
import androidx.leanback.widget.GuidedAction
import dev.sorta.tv.R
import dev.sorta.tv.data.LocaleChoice
import dev.sorta.tv.data.LocalePreference

/**
 * Settings host. Single page for now — language picker — but a
 * dedicated activity gives the toolbar entry on the browse screen
 * a stable target and reserves room to add more options later
 * (theme, default external player, etc.).
 */
class SettingsActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (savedInstanceState == null) {
            GuidedStepSupportFragment.addAsRoot(this, LanguagePickerStep(), android.R.id.content)
        }
    }
}

/**
 * Guided-step page listing the supported [LocaleChoice]s. Selecting
 * one persists it under `LocalePreference`, applies it immediately
 * via `AppCompatDelegate.setApplicationLocales`, and finishes the
 * activity. AppCompat recreates the existing browse activity so
 * resources re-resolve under the new locale automatically.
 */
class LanguagePickerStep : GuidedStepSupportFragment() {

    override fun onCreateGuidance(savedInstanceState: Bundle?): GuidanceStylist.Guidance =
        GuidanceStylist.Guidance(
            getString(R.string.settings_language_title),
            getString(R.string.settings_language_subtitle),
            getString(R.string.settings_section_app),
            null,
        )

    override fun onCreateActions(
        actions: MutableList<GuidedAction>,
        savedInstanceState: Bundle?,
    ) {
        val current = LocalePreference(requireContext()).get()
        for (choice in LocaleChoice.entries) {
            actions.add(
                GuidedAction.Builder(requireContext())
                    .id(choice.ordinal.toLong())
                    .title(labelFor(choice))
                    .checked(choice == current)
                    .checkSetId(GuidedAction.DEFAULT_CHECK_SET_ID)
                    .build(),
            )
        }
    }

    override fun onGuidedActionClicked(action: GuidedAction) {
        val choice = LocaleChoice.entries.firstOrNull { it.ordinal.toLong() == action.id }
            ?: return
        val ctx = requireContext()
        LocalePreference(ctx).set(choice)
        val locales = when (choice) {
            LocaleChoice.SYSTEM -> LocaleListCompat.getEmptyLocaleList()
            else -> LocaleListCompat.forLanguageTags(choice.tag)
        }
        AppCompatDelegate.setApplicationLocales(locales)
        requireActivity().finish()
    }

    private fun labelFor(choice: LocaleChoice): String = when (choice) {
        LocaleChoice.SYSTEM -> getString(R.string.settings_language_system)
        LocaleChoice.EN -> getString(R.string.settings_language_en)
        LocaleChoice.PT_BR -> getString(R.string.settings_language_pt_br)
    }
}
