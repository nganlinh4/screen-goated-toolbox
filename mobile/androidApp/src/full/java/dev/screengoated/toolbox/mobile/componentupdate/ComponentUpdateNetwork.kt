package dev.screengoated.toolbox.mobile.componentupdate

import java.util.concurrent.TimeUnit
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject

internal object ComponentUpdateNetwork {
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .build()

    fun fetchHighest(minimumSequence: Long, hostVersion: String): ComponentCatalogCandidate? {
        val release = downloadJson(RELEASE_API, MAXIMUM_RELEASE_BYTES)
        val assetsJson = release.getJSONArray("assets")
        require(assetsJson.length() <= 256) { "Component catalog release has too many assets" }
        val assets = buildList {
            for (index in 0 until assetsJson.length()) {
                val value = assetsJson.getJSONObject(index)
                add(
                    RemoteAsset(
                        name = value.getString("name"),
                        size = value.getLong("size"),
                        url = value.getString("browser_download_url"),
                    ),
                )
            }
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
                val catalog = downloadExact(asset, MAXIMUM_CATALOG_BYTES.toLong())
                require(sha256(catalog).startsWith(digestPrefix))
                val signature = downloadExact(signatureAsset, 64L)
                val verified = verifyComponentCatalog(
                    ComponentUpdateRuntime.context(),
                    catalog,
                    signature,
                    hostVersion,
                )
                require(verified.sequence == sequence)
                ComponentCatalogCandidate(asset.name, catalog, signature, verified)
            }.getOrNull()
        }
    }

    private fun downloadJson(url: String, maximum: Long): JSONObject =
        JSONObject(download(url, maximum, null).toString(Charsets.UTF_8))

    private fun downloadExact(asset: RemoteAsset, maximum: Long): ByteArray {
        require(asset.size in 1..maximum) { "Component catalog asset size is invalid" }
        return download(asset.url, asset.size, asset.size)
    }

    private fun download(url: String, maximum: Long, expected: Long?): ByteArray {
        val request = Request.Builder()
            .url(url)
            .header("User-Agent", "ScreenGoatedToolbox-ComponentCatalog")
            .build()
        client.newCall(request).execute().use { response ->
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

private data class RemoteAsset(val name: String, val size: Long, val url: String)

private val CATALOG_NAME =
    Regex("^sgt-component-catalog-v(\\d{6})-([0-9a-fA-F]{16})\\.json$")
private const val MAXIMUM_RELEASE_BYTES = 2L * 1024L * 1024L
private const val RELEASE_API =
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases/" +
        "tags/sgt-runtime-bundles"
