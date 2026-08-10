package dev.screengoated.toolbox.mobile.service.moonshine

import android.content.Context
import dev.screengoated.toolbox.mobile.componentupdate.ComponentUpdateCatalog
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import java.net.URI

internal object MoonshineModelDelivery {
    const val ASSET_NAME = "moonshine-model-delivery.json"

    fun load(context: Context): Map<String, MoonshineModelBundle> {
        val updated = ComponentUpdateCatalog.contract(
            "android-moonshine-models-v1",
            setOf("android-arm64"),
        )
        if (updated != null) return parse(updated.toString())
        return context.assets.open(ASSET_NAME).bufferedReader(Charsets.UTF_8).use { reader ->
            parse(reader.readText())
        }
    }

    internal fun parse(source: String): Map<String, MoonshineModelBundle> {
        val root = Json.parseToJsonElement(source).jsonObject
        check(root["schemaVersion"]!!.jsonPrimitive.int == 1) {
            "Unsupported Moonshine model delivery schema"
        }
        check(root["releaseTag"]!!.jsonPrimitive.content == RELEASE_TAG) {
            "Moonshine models must use the append-only runtime-bundles release"
        }
        val variants = root["variants"]!!.jsonArray.map { element ->
            val entry = element.jsonObject
            val id = entry["id"]!!.jsonPrimitive.content
            val asset = entry["asset"]!!.jsonPrimitive.content
            val byteCount = entry["sizeBytes"]!!.jsonPrimitive.long
            val sha256 = entry["sha256"]!!.jsonPrimitive.content
            val downloadUrl = entry["downloadUrl"]!!.jsonPrimitive.content
            check(id.matches(SAFE_ID)) { "Invalid Moonshine model id" }
            check(byteCount > 0) { "Invalid Moonshine model bundle size" }
            check(sha256.matches(SHA256)) { "Invalid Moonshine model bundle SHA-256" }
            check(asset == "sgt-moonshine-model-$id-${sha256.take(16)}.zip") {
                "Moonshine model asset is not content-addressed"
            }
            val uri = URI(downloadUrl)
            check(
                uri.scheme == "https" &&
                    uri.host == RELEASE_HOST &&
                    uri.port == -1 &&
                    uri.rawQuery == null &&
                    uri.rawFragment == null &&
                    uri.path == "$RELEASE_PATH_PREFIX$RELEASE_TAG/$asset"
            ) {
                "Moonshine model bundle URL is not immutable"
            }
            val bundle = MoonshineModelBundle(asset, byteCount, sha256, downloadUrl)
            id to bundle
        }
        val byId = variants.toMap()
        check(byId.size == variants.size) { "Duplicate Moonshine model delivery id" }
        val expectedIds = MoonshineLanguage.entries.map(MoonshineLanguage::modelName).toSet()
        check(byId.keys == expectedIds) { "Moonshine model delivery variants do not match this build" }
        return byId
    }

    private const val RELEASE_TAG = "sgt-runtime-bundles"
    private const val RELEASE_HOST = "github.com"
    private const val RELEASE_PATH_PREFIX = "/nganlinh4/screen-goated-toolbox/releases/download/"
    private val SAFE_ID = Regex("[a-z0-9-]{1,80}")
    private val SHA256 = Regex("[0-9a-f]{64}")
}
