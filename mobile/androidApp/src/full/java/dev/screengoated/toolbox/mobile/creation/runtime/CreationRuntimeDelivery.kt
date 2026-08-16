package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.componentupdate.ComponentUpdateCatalog
import org.json.JSONObject

internal data class CreationRuntimeEntry(
    val role: String,
    val archivePath: String,
    val installPath: String,
    val sizeBytes: Long,
    val sha256: String,
)

internal data class CreationRuntimeDelivery(
    val version: String,
    val factoryClass: String,
    val asset: String,
    val downloadUrl: String,
    val sizeBytes: Long,
    val sha256: String,
    val entries: List<CreationRuntimeEntry>,
) {
    fun entry(role: String): CreationRuntimeEntry =
        requireNotNull(entries.singleOrNull { it.role == role }) {
            "Creation runtime manifest has no unique $role entry"
        }
}

internal fun loadCreationRuntimeDelivery(context: Context): CreationRuntimeDelivery? {
    val root = ComponentUpdateCatalog.contract("creation-runtime-v1", setOf("multi"))
        ?: loadCreationRuntimeManifest(context)
        ?: return null
    return parseCreationRuntimeDelivery(root)
}

internal fun parseCreationRuntimeDelivery(root: JSONObject): CreationRuntimeDelivery {
    require(root.requireString("hostVersion") == BuildConfig.CANONICAL_APP_VERSION) {
        "Creation runtime manifest targets another app version"
    }
    val android = root.requireObject("android")
    val full = android.requireObject("full")
    val entriesJson = android.getJSONArray("entries")
    require(entriesJson.length() in 1..MAXIMUM_DELIVERY_ENTRIES) {
        "Creation runtime manifest has too many entries"
    }
    val entries = buildList {
        for (index in 0 until entriesJson.length()) {
            val entry = entriesJson.getJSONObject(index)
            add(
                CreationRuntimeEntry(
                    role = entry.requireString("role"),
                    archivePath = entry.requireSafeRelativePath("archivePath"),
                    installPath = entry.requireSafeRelativePath("installPath"),
                    sizeBytes = entry.requirePositiveLong("sizeBytes"),
                    sha256 = entry.requireSha256("sha256"),
                ),
            )
        }
    }
    require(entries.map { it.archivePath }.distinct().size == entries.size)
    require(entries.map { it.installPath }.distinct().size == entries.size)
    entries.fold(0L) { total, entry ->
        require(entry.sizeBytes <= MAXIMUM_DELIVERY_BYTES - total) {
            "Creation runtime manifest expands beyond its limit"
        }
        total + entry.sizeBytes
    }

    return CreationRuntimeDelivery(
        version = root.requireFileName("version"),
        factoryClass = android.requireString("factoryClass").also {
            require(FACTORY_CLASS.matches(it)) { "Invalid creation runtime factory class" }
        },
        asset = full.requireFileName("asset"),
        downloadUrl = full.requireString("downloadUrl"),
        sizeBytes = full.requirePositiveLong("sizeBytes"),
        sha256 = full.requireSha256("sha256"),
        entries = entries,
    ).also {
        val expectedAsset = "sgt-creation-runtime-android-arm64-${it.sha256.take(16)}.zip"
        require(it.asset == expectedAsset) {
            "Creation runtime asset is not content-addressed"
        }
        require(
            creationRuntimeDownloadUrlIsImmutable(
                downloadUrl = it.downloadUrl,
                asset = it.asset,
                allowStaging = BuildConfig.DEBUG,
            ),
        ) {
            "Creation runtime URL is not immutable"
        }
        it.entry(ROLE_FACTORY_DEX)
        it.entry(ROLE_NATIVE_LIBRARY)
    }
}

internal fun creationRuntimeDownloadUrlIsImmutable(
    downloadUrl: String,
    asset: String,
    allowStaging: Boolean,
): Boolean =
    downloadUrl == RUNTIME_BUNDLES_PREFIX + asset ||
        (allowStaging && downloadUrl == RUNTIME_STAGING_PREFIX + asset)

private fun JSONObject.requireObject(name: String): JSONObject =
    getJSONObject(name)

private fun JSONObject.requireString(name: String): String =
    getString(name).trim().also { require(it.isNotEmpty()) { "Missing $name" } }

private fun JSONObject.requirePositiveLong(name: String): Long =
    getLong(name).also { require(it > 0L) { "$name must be positive" } }

private fun JSONObject.requireSha256(name: String): String =
    requireString(name).lowercase().also {
        require(it.length == 64 && it.all(Char::isHexDigit)) { "$name is not SHA-256" }
    }

private fun JSONObject.requireFileName(name: String): String =
    requireString(name).also {
        require('/' !in it && '\\' !in it && it != "." && it != "..") {
            "$name must be a file name"
        }
    }

private fun JSONObject.requireSafeRelativePath(name: String): String =
    requireString(name).replace('\\', '/').also {
        require(
            !it.startsWith('/') &&
                it.split('/').none { part -> part.isEmpty() || part == "." || part == ".." },
        ) { "$name must be a safe relative path" }
    }

private fun Char.isHexDigit(): Boolean =
    this in '0'..'9' || this in 'a'..'f'

internal const val ROLE_FACTORY_DEX = "factory_dex"
internal const val ROLE_NATIVE_LIBRARY = "native_library"
private val FACTORY_CLASS =
    Regex("""[A-Za-z_$][A-Za-z0-9_$]*(\.[A-Za-z_$][A-Za-z0-9_$]*)+""")
private const val MAXIMUM_DELIVERY_ENTRIES = 64
private const val MAXIMUM_DELIVERY_BYTES = 1024L * 1024 * 1024
private const val RUNTIME_BUNDLES_PREFIX =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/"
private const val RUNTIME_STAGING_PREFIX =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-staging/"
