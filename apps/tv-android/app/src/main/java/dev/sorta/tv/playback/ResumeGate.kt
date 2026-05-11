package dev.sorta.tv.playback

import android.app.AlertDialog
import android.content.Context
import dev.sorta.tv.R
import dev.sorta.tv.data.WatchHistory
import java.io.File

/**
 * Central entry point every "open the player" callsite routes
 * through. Keeps the watched/near-end/in-progress decision logic
 * in one place so the three launching surfaces (browse, search,
 * series-episodes) behave identically.
 *
 *   ResumeGate.launch(context, history, launcher, file, mediaKey)
 *
 * Behaviour:
 *   - No prior progress, or position 0 → launch from 0 immediately.
 *   - Watched, or ≥95% through → launch from 0 immediately (silent
 *     restart; the user already finished this).
 *   - Mid-playback → show an AlertDialog with two actions:
 *     "Resume from H:MM:SS" (default focus) and "Start over".
 *     Cancel/back closes the dialog without launching, matching
 *     standard Android cancellation semantics.
 */
object ResumeGate {

    /**
     * Look up progress for [mediaKey] and either launch immediately
     * or show the confirmation dialog before launching.
     */
    fun launch(
        context: Context,
        history: WatchHistory,
        launcher: PlayerLauncher,
        file: File,
        mediaKey: String,
    ) {
        val progress = history.progressFor(mediaKey)
        when (val decision = ResumePolicy.decide(progress)) {
            ResumeDecision.Fresh,
            ResumeDecision.RestartFromZero -> launcher.launch(file, mediaKey, startPositionMs = 0L)
            is ResumeDecision.Resume -> showResumeDialog(
                context = context,
                positionMs = decision.positionMs,
                onResume = { launcher.launch(file, mediaKey, startPositionMs = decision.positionMs) },
                onRestart = { launcher.launch(file, mediaKey, startPositionMs = 0L) },
            )
        }
    }

    private fun showResumeDialog(
        context: Context,
        positionMs: Long,
        onResume: () -> Unit,
        onRestart: () -> Unit,
    ) {
        val label = formatPosition(positionMs)
        AlertDialog.Builder(context)
            .setTitle(R.string.resume_dialog_title)
            .setMessage(context.getString(R.string.resume_dialog_message, label))
            // The "positive" button is the default-focused action on
            // a TV remote, so resume goes there (it's the answer the
            // dialog title is asking for).
            .setPositiveButton(context.getString(R.string.resume_dialog_continue, label)) { d, _ ->
                d.dismiss()
                onResume()
            }
            .setNegativeButton(R.string.resume_dialog_start_over) { d, _ ->
                d.dismiss()
                onRestart()
            }
            .setCancelable(true)
            .show()
    }

    /**
     * Format milliseconds as `H:MM:SS` (or `MM:SS` under an hour).
     * Visible-for-test so the formatter is testable without an
     * Android runtime.
     */
    internal fun formatPosition(ms: Long): String {
        val totalSeconds = (ms / 1000).coerceAtLeast(0)
        val seconds = totalSeconds % 60
        val minutes = (totalSeconds / 60) % 60
        val hours = totalSeconds / 3600
        return if (hours > 0) {
            "%d:%02d:%02d".format(hours, minutes, seconds)
        } else {
            "%d:%02d".format(minutes, seconds)
        }
    }
}
