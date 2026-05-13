package dev.sorta.tv.data

/**
 * Predicates that mirror Sorta's on-disk filtering rules. See
 * `docs/disk-format.md` — "Special files readers should ignore".
 *
 * Operates on bare names (no path separators). Pure JVM, no Android
 * dependencies, so it can be unit-tested without instrumentation.
 */
object DiskFormat {

    private val VIDEO_EXTENSIONS = setOf(
        "mkv", "mp4", "avi", "mov", "wmv", "m4v", "webm",
    )

    private val OS_JUNK_NAMES = setOf(
        "System Volume Information",
        ".Trash",
        ".Trashes",
        ".Spotlight-V100",
        ".fseventsd",
        "lost+found",
        // Sorta's own internal cache; readers shouldn't recurse.
        "poster",
    )

    /** True if the file's extension is a recognized video container. */
    fun isVideoFile(name: String): Boolean {
        val dot = name.lastIndexOf('.')
        if (dot < 0 || dot == name.lastIndex) return false
        val ext = name.substring(dot + 1).lowercase()
        return ext in VIDEO_EXTENSIONS
    }

    /**
     * True if the reader should hide this file or directory entry.
     * Covers Sorta's internal markers (`*.original.*`,
     * `*.compressing.*`), the poster cache, and OS junk directories
     * (Recycle Bin, macOS metadata, etc.).
     */
    fun isHiddenForReader(name: String): Boolean {
        if (name.isEmpty()) return false
        // Recycle Bin sits at the root of every Windows drive.
        if (name.startsWith("$")) return true
        if (name in OS_JUNK_NAMES) return true
        // `Foo.original.mkv` / `Foo.compressing.mkv` — pre-compression
        // backup or in-flight encode. Match anywhere a `.original.` or
        // `.compressing.` segment exists between basename and extension.
        if (name.contains(".original.")) return true
        if (name.contains(".compressing.")) return true
        return false
    }
}
