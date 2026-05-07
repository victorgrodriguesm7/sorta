package dev.sorta.tv.usb

import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.net.Uri
import androidx.documentfile.provider.DocumentFile
import java.io.File

/**
 * Storage Access Framework fallback for vendor builds where direct
 * `/storage/usb/` access is denied. Flow:
 *
 *  1. UI builds [openDocumentTreeIntent] and starts it for result.
 *  2. On success, [adoptTree] persists the tree URI grant and locates
 *     `sorta.db` under it.
 *  3. [stageDatabase] copies the DB to internal cache so
 *     [android.database.sqlite.SQLiteDatabase.OPEN_READONLY] can open
 *     it via a real file path (SQLite can't read content:// streams
 *     directly).
 *  4. The tree URI is remembered in [SharedPreferences] so subsequent
 *     boots skip step 1.
 *
 * Posters and video files are still served straight from the tree
 * URI via DocumentFile / ContentResolver — only the SQLite catalog
 * needs to be staged locally.
 */
object SafFallback {

    private const val PREFS_NAME = "sorta-tv"
    private const val KEY_TREE_URI = "saf_tree_uri"
    private const val STAGED_DB_NAME = "staged-sorta.db"
    const val DB_FILENAME = "sorta.db"

    /** Build the picker intent the UI should start for result. */
    fun openDocumentTreeIntent(): Intent =
        Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
            .addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION,
            )

    /**
     * Persist the user's tree-URI grant so we don't have to re-prompt
     * on every launch, and remember it under our own preferences key.
     * Returns the [DocumentFile] for the chosen tree, or null if the
     * URI doesn't resolve to a readable directory.
     */
    fun adoptTree(context: Context, treeUri: Uri): DocumentFile? {
        context.contentResolver.takePersistableUriPermission(
            treeUri,
            Intent.FLAG_GRANT_READ_URI_PERMISSION,
        )
        prefs(context).edit().putString(KEY_TREE_URI, treeUri.toString()).apply()
        val tree = DocumentFile.fromTreeUri(context, treeUri) ?: return null
        return if (tree.isDirectory) tree else null
    }

    /** Recover a previously-adopted tree, if the grant is still valid. */
    fun rememberedTree(context: Context): DocumentFile? {
        val raw = prefs(context).getString(KEY_TREE_URI, null) ?: return null
        val uri = Uri.parse(raw)
        val granted = context.contentResolver.persistedUriPermissions
            .any { it.uri == uri && it.isReadPermission }
        if (!granted) {
            prefs(context).edit().remove(KEY_TREE_URI).apply()
            return null
        }
        val tree = DocumentFile.fromTreeUri(context, uri) ?: return null
        return if (tree.isDirectory) tree else null
    }

    /** Forget the saved tree URI — used when the user picks a new drive. */
    fun forgetTree(context: Context) {
        prefs(context).edit().remove(KEY_TREE_URI).apply()
    }

    /**
     * Copy `sorta.db` from [tree] into the app's cache dir and return
     * the local file. Returns null if the tree doesn't have a
     * `sorta.db` at its root.
     */
    fun stageDatabase(context: Context, tree: DocumentFile): File? {
        val dbDoc = tree.findFile(DB_FILENAME) ?: return null
        if (!dbDoc.isFile) return null
        val target = File(context.cacheDir, STAGED_DB_NAME)
        copyDocumentTo(context.contentResolver, dbDoc.uri, target)
        return target
    }

    private fun copyDocumentTo(resolver: ContentResolver, source: Uri, target: File) {
        resolver.openInputStream(source).use { input ->
            requireNotNull(input) { "could not open $source for read" }
            target.outputStream().use { output -> input.copyTo(output) }
        }
    }

    private fun prefs(context: Context): SharedPreferences =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
}
