package dev.sorta.tv.ui

import android.os.Bundle
import android.content.Context
import androidx.core.content.ContextCompat
import androidx.leanback.app.ErrorSupportFragment
import dev.sorta.tv.R

/**
 * Shows a Leanback-styled full-screen error when we can't render the
 * catalog: no plugged-in drive, no sorta.db on the drive, or a
 * schema version newer than this build understands.
 */
class CatalogErrorFragment : ErrorSupportFragment() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = getString(R.string.app_name)
        val args = requireArguments()
        message = args.getString(ARG_MESSAGE)
        imageDrawable = ContextCompat.getDrawable(requireContext(), R.drawable.ic_error)
        setDefaultBackground(true)
    }

    companion object {
        private const val ARG_MESSAGE = "message"

        fun newInstance(message: String): CatalogErrorFragment =
            CatalogErrorFragment().apply {
                arguments = Bundle().apply { putString(ARG_MESSAGE, message) }
            }

        fun forNoDrive(host: Context): CatalogErrorFragment =
            newInstance(host.getString(R.string.error_no_drive))

        fun forMissingDb(host: Context): CatalogErrorFragment =
            newInstance(host.getString(R.string.error_missing_db))

        fun forSchemaTooNew(host: Context, onDisk: Int, known: Int): CatalogErrorFragment =
            newInstance(host.getString(R.string.error_schema_too_new, onDisk, known))
    }
}
