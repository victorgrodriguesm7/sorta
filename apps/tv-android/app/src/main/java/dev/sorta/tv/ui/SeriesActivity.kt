package dev.sorta.tv.ui

import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.fragment.app.FragmentActivity
import dev.sorta.tv.R
import dev.sorta.tv.data.MediaRow
import java.io.File

/**
 * Hosts the per-series browse screen. The user picks an episode here
 * instead of being routed straight to the first one. The activity is
 * intentionally thin — all UI lives in [SeriesFragment].
 */
class SeriesActivity : FragmentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_series)
    }

    companion object {
        const val EXTRA_DRIVE_ROOT = "drive_root"
        const val EXTRA_FOLDER_PATH = "folder_path"
        const val EXTRA_TITLE = "title"
        const val EXTRA_POSTER_PATH = "poster_path"
        const val EXTRA_POSTER_URL = "poster_url"

        fun intentFor(context: Context, driveRoot: File, media: MediaRow): Intent =
            Intent(context, SeriesActivity::class.java).apply {
                putExtra(EXTRA_DRIVE_ROOT, driveRoot.absolutePath)
                putExtra(EXTRA_FOLDER_PATH, media.folderPath)
                putExtra(EXTRA_TITLE, media.title)
                putExtra(EXTRA_POSTER_PATH, media.posterPath)
                putExtra(EXTRA_POSTER_URL, media.posterUrl)
            }
    }
}
