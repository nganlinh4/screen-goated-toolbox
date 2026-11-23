package dev.screengoated.toolbox.mobile.updater

import okhttp3.OkHttpClient
import okhttp3.Request
import okio.Buffer
import java.math.BigInteger
import java.util.concurrent.TimeUnit

internal class AppUpdateSource(
    private val httpClient: OkHttpClient,
    private val publicKey: () -> ByteArray,
) {
    fun fetchLatestRelease(): Result<AndroidUpdateCandidate> = runCatching {
        fetchStableManifest() ?: fetchGitHubFallback()
    }

    private fun fetchStableManifest(): AndroidUpdateCandidate? {
        val payload = fetchBytesOrNull(MANIFEST_URL, MAXIMUM_MANIFEST_BYTES) ?: return null
        val signature = fetchBytes(SIGNATURE_URL, 64)
        require(signature.size == 64) { "App update signature shape is invalid" }
        val publicPoint = publicKey()
        return verifiedStableManifest(payload, signature, publicPoint)
    }

    private fun fetchGitHubFallback(): AndroidUpdateCandidate {
        var latest: AndroidUpdateCandidate? = null
        var ambiguous = false
        for (page in 1..MAXIMUM_RELEASE_PAGES) {
            val payload = fetchBytes(
                "$RELEASES_URL?per_page=$RELEASES_PER_PAGE&page=$page",
                MAXIMUM_GITHUB_RESPONSE_BYTES,
            )
            val releases = githubReleasePage(payload)
            require(releases.count <= RELEASES_PER_PAGE) { "GitHub release page exceeded its entry limit" }
            for (candidate in releases.candidates) {
                val ordering = latest?.let { compareCanonicalVersions(candidate.version,it.version) } ?: 1
                when {
                    ordering > 0 -> {
                        latest = candidate
                        ambiguous = false
                    }
                    ordering == 0 && latest != candidate -> ambiguous = true
                }
            }
            if (releases.count < RELEASES_PER_PAGE) {
                require(!ambiguous) { "Highest stable GitHub release has conflicting metadata" }
                return latest ?: error("No compatible stable GitHub release was found")
            }
        }
        error("GitHub release census exceeded its page limit before completion")
    }

    private fun fetchBytes(url: String, limit: Int): ByteArray {
        return fetchBytesOrNull(url, limit, allowMissing = false)
            ?: error("Required update source is missing")
    }

    private fun fetchBytesOrNull(
        url: String,
        limit: Int,
        allowMissing: Boolean = true,
    ): ByteArray? {
        val request = Request.Builder()
            .url(url)
            .header("User-Agent", "screen-goated-toolbox-android-updater")
            .build()
        val call = httpClient.newCall(request)
        call.timeout().timeout(10, TimeUnit.SECONDS)
        call.execute().use { response ->
            if (allowMissing && response.code == 404) return null
            require(response.isSuccessful) { "Update source failed: HTTP ${response.code}" }
            val body = response.body
            require(body.contentLength() <= limit || body.contentLength() == -1L) {
                "Update source exceeded its size limit"
            }
            val source = body.source()
            val buffer = Buffer()
            while (buffer.size <= limit) {
                val remaining = limit.toLong() + 1L - buffer.size
                if (source.read(buffer, minOf(8_192L, remaining)) == -1L) break
            }
            val bytes = buffer.readByteArray()
            require(bytes.size <= limit) { "Update source exceeded its size limit" }
            return bytes
        }
    }

    private companion object {
        const val MANIFEST_URL =
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/app-update-feed/stable-v1.json"
        const val SIGNATURE_URL =
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/app-update-feed/stable-v1.sig"
        const val RELEASES_URL =
            "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases"
        const val RELEASES_PER_PAGE = 100
        const val MAXIMUM_RELEASE_PAGES = 10
    }
}

private fun compareCanonicalVersions(left: String, right: String): Int {
    val leftParts = left.split('.').map(::BigInteger)
    val rightParts = right.split('.').map(::BigInteger)
    return leftParts.zip(rightParts)
        .firstOrNull { (leftPart, rightPart) -> leftPart != rightPart }
        ?.let { (leftPart, rightPart) -> leftPart.compareTo(rightPart) }
        ?: 0
}

internal fun decodeAppUpdatePublicKey(value: String): ByteArray {
    require(
        value.length == 130 && value.startsWith("04") &&
            value.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' },
    )
    return ByteArray(value.length / 2) { index ->
        value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
    }
}

