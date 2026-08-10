package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import dev.screengoated.toolbox.mobile.componentupdate.ComponentUpdateCatalog
import org.json.JSONObject

internal enum class DownloaderArtifactRole(val wireName: String) {
    YT_DLP("yt_dlp"),
    PYTHON("python"),
    FFMPEG("ffmpeg");

    companion object {
        fun fromWireName(value: String): DownloaderArtifactRole =
            entries.singleOrNull { it.wireName == value }
                ?: error("Unsupported downloader artifact role")
    }
}

internal data class DownloaderRuntimeArtifact(
    val role: DownloaderArtifactRole,
    val asset: String,
    val downloadUrl: String,
    val sizeBytes: Long,
    val sha256: String,
    val entryCount: Int?,
    val uncompressedBytes: Long?,
    val requiredPaths: List<String>,
)

internal data class DownloaderRuntimeDelivery(
    val version: String,
    val abi: String,
    val artifacts: List<DownloaderRuntimeArtifact>,
) {
    fun artifact(role: DownloaderArtifactRole): DownloaderRuntimeArtifact =
        artifacts.single { it.role == role }

    val identity: String
        get() = buildString {
            append(version)
            artifacts.sortedBy { it.role.ordinal }.forEach {
                append('|')
                append(it.role.wireName)
                append(':')
                append(it.sha256)
            }
        }
}

internal fun loadDownloaderRuntimeDelivery(context: Context): DownloaderRuntimeDelivery? =
    runCatching {
        ComponentUpdateCatalog.contract(
            "android-downloader-runtime-v1",
            setOf("android-full-arm64"),
        )?.let { return@runCatching parseDownloaderRuntimeDelivery(it.toString()) }
        context.assets.open("downloader-runtime/delivery.json").bufferedReader().use {
            parseDownloaderRuntimeDelivery(it.readText())
        }
    }.getOrNull()

internal fun parseDownloaderRuntimeDelivery(raw: String): DownloaderRuntimeDelivery {
    val root = JSONObject(raw)
    require(root.getInt("schemaVersion") == 1) { "Unsupported downloader delivery schema" }
    val version = root.requiredString("version")
    require(version.matches(Regex("[0-9A-Za-z._-]+"))) { "Invalid downloader version" }
    val abi = root.requiredString("abi")
    require(abi == "arm64-v8a") { "Unsupported downloader ABI" }

    val array = root.getJSONArray("artifacts")
    val artifacts = buildList {
        for (index in 0 until array.length()) {
            val value = array.getJSONObject(index)
            val role = DownloaderArtifactRole.fromWireName(value.requiredString("role"))
            val asset = value.requiredString("asset")
            require(isSafeRelativePath(asset) && '/' !in asset) { "Unsafe downloader asset" }
            val url = value.requiredString("downloadUrl")
            val sizeBytes = value.getLong("sizeBytes")
            require(sizeBytes > 0L) { "Invalid downloader artifact size" }
            val sha256 = value.requiredString("sha256")
            require(sha256.matches(Regex("[0-9a-f]{64}"))) { "Invalid downloader SHA-256" }
            require(url.endsWith("/$asset")) { "Downloader asset URL differs" }

            val entryCount = value.optInt("entryCount", 0).takeIf { it > 0 }
            val uncompressedBytes = value.optLong("uncompressedBytes", 0L).takeIf { it > 0L }
            val requiredPaths = value.optJSONArray("requiredPaths")?.let { paths ->
                buildList {
                    for (pathIndex in 0 until paths.length()) {
                        val path = paths.getString(pathIndex)
                        require(isSafeRelativePath(path)) { "Unsafe downloader installed path" }
                        add(path)
                    }
                }
            }.orEmpty()

            if (role == DownloaderArtifactRole.YT_DLP) {
                require(url.matches(OFFICIAL_YT_DLP_URL)) {
                    "yt-dlp must use an immutable official release"
                }
                val release = url.substringAfter("/download/").substringBefore('/')
                require(version.startsWith(release)) { "yt-dlp delivery version differs" }
                require(entryCount == null && uncompressedBytes == null) {
                    "yt-dlp must be delivered as a direct file"
                }
            } else {
                require(url.startsWith(SGT_RUNTIME_BUNDLE_URL)) {
                    "Downloader archive must use sgt-runtime-bundles"
                }
                require(asset.startsWith("sgt-downloader-") && asset.contains(sha256.take(12))) {
                    "Downloader archive asset is not uniquely identified"
                }
                require(entryCount != null && uncompressedBytes != null && requiredPaths.isNotEmpty()) {
                    "Downloader archive extraction contract is incomplete"
                }
            }
            add(
                DownloaderRuntimeArtifact(
                    role = role,
                    asset = asset,
                    downloadUrl = url,
                    sizeBytes = sizeBytes,
                    sha256 = sha256,
                    entryCount = entryCount,
                    uncompressedBytes = uncompressedBytes,
                    requiredPaths = requiredPaths,
                ),
            )
        }
    }
    require(artifacts.map { it.role }.toSet() == DownloaderArtifactRole.entries.toSet()) {
        "Downloader delivery must contain each runtime artifact exactly once"
    }
    require(artifacts.size == DownloaderArtifactRole.entries.size) {
        "Downloader delivery repeats an artifact"
    }
    return DownloaderRuntimeDelivery(version, abi, artifacts)
}

internal fun isSafeRelativePath(path: String): Boolean {
    if (path.isBlank() || path.startsWith('/') || path.startsWith('\\')) return false
    if ('\\' in path || ':' in path) return false
    return path.split('/').none { it.isBlank() || it == "." || it == ".." }
}

private fun JSONObject.requiredString(name: String): String =
    getString(name).takeIf(String::isNotBlank) ?: error("Missing downloader $name")

private val OFFICIAL_YT_DLP_URL = Regex(
    "https://github\\.com/yt-dlp/yt-dlp/releases/download/[0-9.]+/yt-dlp",
)

private const val SGT_RUNTIME_BUNDLE_URL =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/" +
        "sgt-runtime-bundles/sgt-downloader-"
