package dev.sorta.tv

import android.app.Application
import android.os.StrictMode

/**
 * Process-wide setup. Currently just relaxes one StrictMode default:
 * `detectFileUriExposure`, which is implicitly on for apps targeting
 * API 24+ and throws [android.os.FileUriExposedException] when a
 * `file://` URI is shared with another app via an Intent.
 *
 * We deliberately want `file://` for video playback because the
 * external players we target on the deployment box (VLC, MX Player)
 * accept it directly — see the "player intent compatibility" note in
 * `apps/tv-android/PLAN.md`. Switching to `FileProvider` would force
 * per-recipient URI grants and break players that only accept file
 * paths, so we keep file:// and silence the platform check.
 */
class SortaTvApp : Application() {
    override fun onCreate() {
        super.onCreate()
        StrictMode.setVmPolicy(StrictMode.VmPolicy.Builder().build())
    }
}
