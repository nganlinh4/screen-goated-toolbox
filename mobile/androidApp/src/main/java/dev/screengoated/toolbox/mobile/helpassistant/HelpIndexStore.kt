package dev.screengoated.toolbox.mobile.helpassistant

import android.content.Context
import dev.screengoated.toolbox.mobile.BuildConfig
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.security.MessageDigest
import java.util.zip.GZIPInputStream

internal class HelpIndexStore(
    private val context: Context,
    private val httpClient: OkHttpClient,
) {
    fun load(): List<ChunkEntry> {
        val delivery = parseDelivery(context.assets.open(DELIVERY_ASSET).use { it.readBytes() })
        return runCatching { loadSelected(delivery) }.getOrElse { selectedError ->
            runCatching { loadLastGood() }
                .getOrElse { cachedError ->
                    throw IOException(
                        "Failed to fetch help data: ${selectedError.message}; " +
                            "cached copy: ${cachedError.message}",
                        selectedError,
                    )
                }
        }
    }

    private fun loadSelected(delivery: HelpIndexDelivery): List<ChunkEntry> {
        val selected = File(cacheRoot(), delivery.asset)
        if (selected.isFile && selected.length() == delivery.sizeBytes.toLong()) {
            runCatching { return verifyAndParse(delivery, selected.readBytes()) }
        }
        val request = Request.Builder()
            .url(delivery.downloadUrl)
            .header("User-Agent", "ScreenGoatedToolbox-HelpData")
            .build()
        val compressed = httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw IOException("HTTP ${response.code}")
            response.body.byteStream().use { input ->
                val bytes = input.readNBytes(delivery.sizeBytes + 1)
                if (bytes.size != delivery.sizeBytes) throw IOException("Invalid response length")
                bytes
            }
        }
        val entries = verifyAndParse(delivery, compressed)
        cacheRoot().mkdirs()
        atomicWrite(selected, compressed)
        val expanded = expand(delivery, compressed)
        atomicWrite(lastGoodFile(), expanded)
        atomicWrite(lastGoodDigestFile(), delivery.expandedSha256.encodeToByteArray())
        return entries
    }

    private fun verifyAndParse(
        delivery: HelpIndexDelivery,
        compressed: ByteArray,
    ): List<ChunkEntry> {
        require(compressed.size == delivery.sizeBytes && sha256(compressed) == delivery.sha256) {
            "Invalid compressed help data identity"
        }
        val expanded = expand(delivery, compressed)
        require(
            JSONObject(expanded.toString(Charsets.UTF_8)).getJSONArray("entries").length() ==
                delivery.entryCount,
        ) { "Invalid help data entry count" }
        val entries = parseHelpIndex(expanded, "android")
        return entries
    }

    private fun cacheRoot(): File = File(context.filesDir, "help-assistant")

    private fun lastGoodFile(): File = File(cacheRoot(), "last-good.json")

    private fun lastGoodDigestFile(): File = File(cacheRoot(), "last-good.sha256")

    private fun loadLastGood(): List<ChunkEntry> {
        val data = lastGoodFile()
        require(data.isFile && data.length() in 1L..MAXIMUM_BYTES.toLong())
        val bytes = data.readBytes()
        val expected = lastGoodDigestFile().readText()
        require(expected.isSha256() && sha256(bytes) == expected) { "Invalid cached help data identity" }
        return parseHelpIndex(bytes, "android")
    }
}

internal data class HelpIndexDelivery(
    val version: String,
    val asset: String,
    val downloadUrl: String,
    val sizeBytes: Int,
    val sha256: String,
    val expandedSizeBytes: Int,
    val expandedSha256: String,
    val entryCount: Int,
)

internal fun parseDelivery(bytes: ByteArray): HelpIndexDelivery {
    val root = JSONObject(bytes.toString(Charsets.UTF_8))
    require(root.getInt("schemaVersion") == 1)
    val version = root.getString("version")
    val value = root.getJSONObject("helpIndex")
    val delivery = HelpIndexDelivery(
        version = version,
        asset = value.getString("asset"),
        downloadUrl = value.getString("downloadUrl"),
        sizeBytes = value.getInt("sizeBytes"),
        sha256 = value.getString("sha256"),
        expandedSizeBytes = value.getInt("expandedSizeBytes"),
        expandedSha256 = value.getString("expandedSha256"),
        entryCount = value.getInt("entryCount"),
    )
    val productionUrl = PRODUCTION_PREFIX + delivery.asset
    val stagingUrl = STAGING_PREFIX + delivery.asset
    require(value.getString("id") == "help-index" && value.getString("format") == "json-gzip")
    require(delivery.sha256.isSha256() && delivery.expandedSha256.isSha256())
    require(delivery.asset == "help-index-v$version-${delivery.sha256.take(16)}.json.gz")
    require(delivery.downloadUrl == productionUrl || BuildConfig.DEBUG && delivery.downloadUrl == stagingUrl)
    require(delivery.sizeBytes in 1..MAXIMUM_BYTES)
    require(delivery.expandedSizeBytes in 1..MAXIMUM_BYTES)
    require(delivery.entryCount in 1..MAXIMUM_ENTRIES)
    return delivery
}

internal fun parseHelpIndex(bytes: ByteArray, platform: String): List<ChunkEntry> {
    require(bytes.size <= MAXIMUM_BYTES)
    val root = JSONObject(bytes.toString(Charsets.UTF_8))
    require(root.getInt("schemaVersion") == 1)
    val raw = root.getJSONArray("entries")
    require(raw.length() in 1..MAXIMUM_ENTRIES)
    return buildList {
        for (index in 0 until raw.length()) {
            val value = raw.getJSONObject(index)
            val path = value.getString("path")
            val text = value.getString("text")
            val platforms = value.getJSONArray("platforms")
            require(path.isNotEmpty() && path.length <= 256)
            require(text.isNotEmpty() && text.length <= MAXIMUM_DOCUMENT_CHARS)
            val supported = (0 until platforms.length()).map(platforms::getString)
            require(supported.isNotEmpty() && supported.all { it == "windows" || it == "android" })
            if (platform in supported) add(ChunkEntry(path, text))
        }
    }
}

private fun expand(delivery: HelpIndexDelivery, compressed: ByteArray): ByteArray {
    val output = ByteArrayOutputStream(delivery.expandedSizeBytes)
    GZIPInputStream(ByteArrayInputStream(compressed)).use { input ->
        val buffer = ByteArray(8192)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            output.write(buffer, 0, count)
            require(output.size() <= delivery.expandedSizeBytes) { "Expanded help data is too large" }
        }
    }
    return output.toByteArray().also { expanded ->
        require(
            expanded.size == delivery.expandedSizeBytes &&
                sha256(expanded) == delivery.expandedSha256,
        ) { "Invalid expanded help data identity" }
    }
}

private fun atomicWrite(target: File, bytes: ByteArray) {
    target.parentFile?.mkdirs()
    val temporary = File(target.parentFile, "${target.name}.partial")
    temporary.writeBytes(bytes)
    if (!target.exists()) {
        check(temporary.renameTo(target))
        return
    }
    val backup = File(target.parentFile, "${target.name}.previous")
    backup.delete()
    check(target.renameTo(backup))
    if (!temporary.renameTo(target)) {
        backup.renameTo(target)
        error("Could not publish help data cache")
    }
    backup.delete()
}

private fun sha256(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

private fun String.isSha256(): Boolean =
    length == 64 && all { it.isDigit() || it in 'a'..'f' }

private const val DELIVERY_ASSET = "help-assistant/delivery.json"
private const val PRODUCTION_PREFIX =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/"
private const val STAGING_PREFIX =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-staging/"
private const val MAXIMUM_BYTES = 4 * 1024 * 1024
private const val MAXIMUM_ENTRIES = 128
private const val MAXIMUM_DOCUMENT_CHARS = 128 * 1024
