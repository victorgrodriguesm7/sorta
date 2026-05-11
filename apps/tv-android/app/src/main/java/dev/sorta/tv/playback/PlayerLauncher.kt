package dev.sorta.tv.playback

import android.app.Activity
import android.content.Intent
import android.net.Uri
import androidx.activity.result.ActivityResult
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.fragment.app.Fragment
import dev.sorta.tv.data.WatchHistory
import java.io.File

/**
 * Owns the registerForActivityResult plumbing for launching an
 * external video player and recording the returned position.
 *
 * Each launching Fragment instantiates one of these as a field. The
 * registerForActivityResult call has to happen before the Fragment
 * is STARTED, which is satisfied by field initialisation.
 *
 * VLC for Android and MX Player both accept a `position` extra to
 * resume from and return a final position when launched with
 * `startActivityForResult`. The exact extra names differ slightly,
 * so we set both and read both back.
 */
class PlayerLauncher(
    fragment: Fragment,
    /**
     * Deferred lookup so the launcher itself can be constructed
     * during field initialisation (which fires before `onAttach`,
     * where `requireContext()` would otherwise throw). We resolve
     * the [WatchHistory] only at launch / result time.
     */
    private val historyProvider: () -> WatchHistory,
) {
    private var pendingKey: String? = null

    // registerForActivityResult MUST run before the fragment reaches
    // its CREATED lifecycle state. Initialising this property at
    // construction time (rather than via `by lazy`) guarantees that.
    private val launcher: ActivityResultLauncher<Intent> =
        fragment.registerForActivityResult(
            ActivityResultContracts.StartActivityForResult(),
            ::onResult,
        )

    /**
     * Launch [file] in the user's chosen external player, resuming
     * from [startPositionMs] (or 0 for a fresh start). The caller is
     * responsible for deciding the start position — typically by
     * routing through [ResumeGate], which handles the "watched →
     * restart from 0" and "in-progress → confirm" rules.
     */
    fun launch(file: File, mediaKey: String, startPositionMs: Long = 0L) {
        pendingKey = mediaKey
        val request = PlaybackIntent.build(file)
        val data = Uri.fromFile(File(request.filePath))
        val intent = Intent(request.action)
            .setDataAndType(data, request.mimeType)
            .addFlags(request.flags)
            // VLC: read on input, written on output.
            // MX Player: same key, same semantics.
            .putExtra("position", startPositionMs)
            // MX Player gate that opts the app into receiving a
            // setResult callback at all. VLC ignores it.
            .putExtra("return_result", true)
        launcher.launch(intent)
    }

    private fun onResult(result: ActivityResult) {
        val key = pendingKey ?: return
        pendingKey = null
        if (result.resultCode != Activity.RESULT_OK) return
        val data = result.data ?: return
        val position = data.longExtraEither("extra_position", "position")
        val duration = data.longExtraEither("extra_duration", "duration")
        val endedByCompletion = data.getStringExtra("end_by") == "playback_completion"

        // Some players don't return position at all (system default
        // video player on most boxes). In that case we have nothing
        // to update and leave the row alone.
        if (position <= 0 && duration <= 0 && !endedByCompletion) return

        val history = historyProvider()
        history.record(key, positionMs = position, durationMs = duration)
        if (endedByCompletion) {
            history.setWatched(key, true)
        }
    }
}

/**
 * Read either of two extra names — VLC uses `extra_position` /
 * `extra_duration`, MX Player uses the unprefixed forms. Numeric
 * extras can land as Int or Long depending on player; getLongExtra
 * coerces only Long, so fall back to getIntExtra.
 */
private fun Intent.longExtraEither(primary: String, fallback: String): Long {
    val pl = getLongExtra(primary, -1L)
    if (pl >= 0) return pl
    val pi = getIntExtra(primary, -1)
    if (pi >= 0) return pi.toLong()
    val fl = getLongExtra(fallback, -1L)
    if (fl >= 0) return fl
    val fi = getIntExtra(fallback, -1)
    if (fi >= 0) return fi.toLong()
    return 0L
}
