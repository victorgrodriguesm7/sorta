package dev.sorta.tv.playback

import java.io.File

/**
 * Pure description of "ask the system to play this video file" — built
 * here without touching the Android `Intent` API so the logic stays
 * unit-testable on the JVM. The `ui` layer turns a [PlaybackRequest]
 * into a real `Intent` and calls `startActivity`.
 *
 * Per `docs/disk-format.md#reading-a-single-linked-file` the playback
 * shape on Android 7.1.1 is:
 *
 *   Intent(ACTION_VIEW).setDataAndType(Uri.fromFile(file), "video/*")
 *
 * VLC and MX Player accept `file://` URIs directly on this API level;
 * if a stricter player ever needs FileProvider URIs we'll layer that
 * in here without changing the UI call sites.
 */
data class PlaybackRequest(
    val action: String,
    val uri: String,
    val mimeType: String,
    val flags: Int,
) {
    companion object {
        const val ACTION_VIEW = "android.intent.action.VIEW"
        const val FLAG_GRANT_READ_URI_PERMISSION = 0x00000001
    }
}

object PlaybackIntent {

    /**
     * Build a [PlaybackRequest] for a single video file. The URI uses
     * the `file://` scheme — sufficient for the players we target on
     * the deployment box (Android 7.1.1, no FileProvider needed yet).
     */
    fun build(file: File): PlaybackRequest = PlaybackRequest(
        action = PlaybackRequest.ACTION_VIEW,
        uri = file.toURI().toString(),
        mimeType = "video/*",
        flags = PlaybackRequest.FLAG_GRANT_READ_URI_PERMISSION,
    )
}
