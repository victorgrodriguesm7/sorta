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
 *   Intent(ACTION_VIEW).setDataAndType(Uri.fromFile(file), "video/ *")
 *
 * VLC and MX Player accept `file://` URIs directly on this API level;
 * if a stricter player ever needs FileProvider URIs we'll layer that
 * in here without changing the UI call sites.
 */
data class PlaybackRequest(
    val action: String,
    /**
     * Absolute path to the video file. Kept as a plain String (not a
     * URI) so this data class stays JVM-testable — the UI layer turns
     * it into a real `android.net.Uri` via `Uri.fromFile(File(path))`,
     * which emits the triple-slash `file:///…` form every Android
     * player parses correctly. (`java.io.File.toURI()` emits the
     * single-slash `file:/…` form, which VLC rejects with a generic
     * "can't reproduce media" error.)
     */
    val filePath: String,
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
     * Build a [PlaybackRequest] for a single video file. The UI layer
     * is responsible for materialising the file path into the
     * `file:///…` URI Android players expect — see the field doc on
     * [PlaybackRequest.filePath].
     */
    fun build(file: File): PlaybackRequest = PlaybackRequest(
        action = PlaybackRequest.ACTION_VIEW,
        // `file.path` (not `absolutePath`) so the string survives
        // verbatim — production callers always hand us already-absolute
        // paths from the catalog, and `absolutePath` would re-anchor
        // relative inputs against the JVM CWD on the test host.
        filePath = file.path,
        mimeType = "video/*",
        flags = PlaybackRequest.FLAG_GRANT_READ_URI_PERMISSION,
    )
}
