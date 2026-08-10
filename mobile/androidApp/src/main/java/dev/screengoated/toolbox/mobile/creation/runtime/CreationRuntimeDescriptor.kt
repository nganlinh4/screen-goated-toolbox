package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.componentupdate.ComponentUpdateCatalog
import dev.screengoated.toolbox.mobile.creation.readCreationBytesBounded
import org.json.JSONObject

internal data class CreationRuntimeProductDescriptor(
    val runtimeVersion: String,
    val features: Set<String>,
)

internal fun loadCreationRuntimeManifest(context: Context): JSONObject? = runCatching {
    context.assets.open(CREATION_RUNTIME_DELIVERY_ASSET).use {
        JSONObject(readCreationBytesBounded(it, MAXIMUM_RUNTIME_MANIFEST_BYTES).decodeToString())
    }
}.getOrNull()

internal fun loadCreationRuntimeProductDescriptor(
    context: Context,
): CreationRuntimeProductDescriptor? = (
    if (BuildConfig.FLAVOR == "full") {
        ComponentUpdateCatalog.contract("creation-runtime-v1", setOf("multi"))
    } else {
        null
    } ?: loadCreationRuntimeManifest(context)
).let { root ->
    root ?: return@let null
    runCatching {
        val version = root.getString("version").trim()
        require(version.isNotEmpty())
        val values = root.getJSONArray("features")
        val features = buildSet {
            for (index in 0 until values.length()) {
                val feature = values.getString(index).trim()
                require(feature.isNotEmpty() && add(feature))
            }
        }
        require(features.isNotEmpty())
        CreationRuntimeProductDescriptor(version, features)
    }.getOrNull()
}

internal fun loadCreationRuntimeFactoryClass(context: Context): String? =
    loadCreationRuntimeManifest(context)
        ?.optJSONObject("android")
        ?.optString("factoryClass")
        ?.trim()
        ?.takeIf(String::isNotEmpty)

private const val CREATION_RUNTIME_DELIVERY_ASSET = "creation-runtime/delivery.json"
internal const val MAXIMUM_RUNTIME_MANIFEST_BYTES = 64L * 1024
