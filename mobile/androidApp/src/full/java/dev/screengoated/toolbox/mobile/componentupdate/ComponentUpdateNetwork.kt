package dev.screengoated.toolbox.mobile.componentupdate

import android.content.Context
import java.util.concurrent.TimeUnit
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import org.json.JSONArray

internal object ComponentUpdateNetwork {
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .build()

    fun fetchHighest(context: Context, minimumSequence: Long, hostVersion: String): ComponentCatalogCandidate? {
        val deadline = System.nanoTime() + TimeUnit.MINUTES.toNanos(2)
        val release = JSONObject(download(RELEASE_API, MAXIMUM_RELEASE_BYTES, null, deadline).toString(Charsets.UTF_8))
        val releaseId = release.getLong("id")
        require(releaseId > 0)
        val assets = collectReleaseAssets { page ->
            val url = "$RELEASE_ASSETS_API/$releaseId/assets?per_page=100&page=$page"
            JSONArray(download(url, MAXIMUM_RELEASE_BYTES, null, deadline).toString(Charsets.UTF_8))
        }
        val signatures = assets.associateBy(RemoteAsset::name)
        return assets.mapNotNull { asset ->
            val match = CATALOG_NAME.matchEntire(asset.name) ?: return@mapNotNull null
            val sequence = match.groupValues[1].toLong()
            if (sequence < minimumSequence) return@mapNotNull null
            Triple(sequence, match.groupValues[2].lowercase(), asset)
        }.sortedByDescending { it.first }.firstNotNullOfOrNull { (sequence, digestPrefix, asset) ->
            runCatching {
                val signatureAsset = signatures[asset.name.removeSuffix(".json") + ".sig"]
                    ?.takeIf { it.size == 64L } ?: error("Catalog signature is missing")
                val catalog = downloadExact(asset, MAXIMUM_CATALOG_BYTES.toLong(), deadline)
                require(sha256(catalog).startsWith(digestPrefix))
                val signature = downloadExact(signatureAsset, 64L, deadline)
                val verified = verifyComponentCatalog(
                    context,
                    catalog,
                    signature,
                    hostVersion,
                )
                require(verified.sequence == sequence)
                ComponentCatalogCandidate(asset.name, catalog, signature, verified)
            }.getOrNull()
        }
    }

    private fun downloadExact(asset: RemoteAsset, maximum: Long, deadline: Long): ByteArray {
        require(asset.size in 1..maximum) { "Component catalog asset size is invalid" }
        return download(asset.url, asset.size, asset.size, deadline)
    }

    private fun download(url: String, maximum: Long, expected: Long?, deadline: Long): ByteArray {
        val remaining = deadline - System.nanoTime()
        check(remaining > 0) { "Component catalog discovery timed out" }
        val request = Request.Builder()
            .url(url)
            .header("User-Agent", "ScreenGoatedToolbox-ComponentCatalog")
            .build()
        client.newBuilder().callTimeout(remaining, TimeUnit.NANOSECONDS).build().newCall(request).execute().use { response ->
            check(response.isSuccessful) { "Component catalog HTTP ${response.code}" }
            val declared = response.body.contentLength()
            check(declared < 0L || declared <= maximum)
            expected?.let { check(declared < 0L || declared == it) }
            val input = response.body.byteStream()
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(64 * 1024)
            var total = 0L
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                total += read
                check(total <= maximum) { "Component catalog response exceeded its limit" }
                output.write(buffer, 0, read)
            }
            expected?.let { check(total == it) }
            return output.toByteArray()
        }
    }
}

internal data class RemoteAsset(val name: String, val size: Long, val url: String)

internal fun collectReleaseAssets(fetchPage: (Int) -> JSONArray): List<RemoteAsset> {
    val assets = mutableListOf<RemoteAsset>()
    var page = 1
    while (true) {
        val entries = fetchPage(page)
        require(entries.length() <= 100) { "Component catalog asset page is too large" }
        for (index in 0 until entries.length()) {
            val value = entries.getJSONObject(index)
            val name = value.getString("name")
            if (CATALOG_NAME.matches(name) || SIGNATURE_NAME.matches(name)) {
                assets.add(RemoteAsset(name, value.getLong("size"), value.getString("browser_download_url")))
            }
        }
        if (entries.length() < 100) return assets
        page = Math.addExact(page, 1)
    }
}

private val CATALOG_NAME =
    Regex("^sgt-component-catalog-v(\\d{6})-([0-9a-fA-F]{16})\\.json$")
private val SIGNATURE_NAME = Regex("^sgt-component-catalog-v\\d{6}-[0-9a-fA-F]{16}\\.sig$")
private const val MAXIMUM_RELEASE_BYTES = 8L * 1024L * 1024L
private const val RELEASE_ASSETS_API =
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases"
private const val RELEASE_API =
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases/" +
        "tags/sgt-runtime-bundles"
