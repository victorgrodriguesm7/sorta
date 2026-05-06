package dev.sorta.tv.ui

import android.os.Bundle
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.lifecycleScope
import dev.sorta.tv.R
import dev.sorta.tv.data.CatalogCheck
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Hosts either the Leanback [BrowseFragment] (happy path) or a
 * [CatalogErrorFragment] (no drive / missing DB / schema too new).
 * The check runs once on create and the result picks the fragment.
 */
class BrowseActivity : FragmentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_browse)
        if (savedInstanceState == null) {
            lifecycleScope.launch {
                val result = withContext(Dispatchers.IO) { CatalogCheck.run() }
                showFor(result)
            }
        }
    }

    private fun showFor(result: CatalogCheck.Result) {
        val fragment = when (result) {
            is CatalogCheck.Result.Ok -> BrowseFragment.newInstance(result.driveRoot)
            CatalogCheck.Result.NoDrive -> CatalogErrorFragment.forNoDrive(this)
            is CatalogCheck.Result.MissingDb -> CatalogErrorFragment.forMissingDb(this)
            is CatalogCheck.Result.SchemaTooNew ->
                CatalogErrorFragment.forSchemaTooNew(this, result.onDisk, result.known)
        }
        supportFragmentManager.beginTransaction()
            .replace(R.id.browse_fragment, fragment)
            .commit()
    }
}
