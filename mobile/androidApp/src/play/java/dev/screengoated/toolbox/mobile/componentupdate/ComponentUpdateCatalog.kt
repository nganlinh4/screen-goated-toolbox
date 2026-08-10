package dev.screengoated.toolbox.mobile.componentupdate

import android.content.Context
import org.json.JSONObject

/** Play native delivery and contract updates remain owned by reviewed Store builds. */
internal object ComponentUpdateCatalog {
    fun loadCached(context: Context) {
        require(context.packageName.isNotBlank())
    }

    fun refreshInBackground(context: Context) {
        require(context.packageName.isNotBlank())
    }

    fun contract(name: String, platforms: Set<String>): JSONObject? {
        require(name.isNotBlank() && platforms.isNotEmpty())
        return null
    }
}
