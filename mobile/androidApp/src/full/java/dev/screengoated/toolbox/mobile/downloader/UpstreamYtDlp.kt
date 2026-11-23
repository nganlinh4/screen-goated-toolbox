package dev.screengoated.toolbox.mobile.downloader

import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.security.MessageDigest
import java.time.LocalDate
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.bouncycastle.openpgp.PGPObjectFactory
import org.bouncycastle.openpgp.PGPPublicKeyRingCollection
import org.bouncycastle.openpgp.PGPSignatureList
import org.bouncycastle.openpgp.PGPUtil
import org.bouncycastle.openpgp.operator.bc.BcKeyFingerprintCalculator
import org.bouncycastle.openpgp.operator.bc.BcPGPContentVerifierBuilderProvider
import org.json.JSONObject

@ConsistentCopyVisibility
internal data class YtDlpVersion private constructor(
    val tag: String,
    private val date: LocalDate,
    private val revision: Int,
) : Comparable<YtDlpVersion> {
    override fun compareTo(other: YtDlpVersion): Int =
        compareValuesBy(this, other, YtDlpVersion::date, YtDlpVersion::revision)

    companion object {
        private val format = Regex("""(\d{4})\.(\d{2})\.(\d{2})(?:\.([1-9]\d{0,5}))?""")

        fun parse(tag: String): YtDlpVersion? {
            val match = format.matchEntire(tag) ?: return null
            val (_, year, month, day, revision) = match.groupValues
            return runCatching {
                YtDlpVersion(tag, LocalDate.of(year.toInt(), month.toInt(), day.toInt()), revision.toIntOrNull() ?: 0)
            }.getOrNull()
        }
    }
}

internal data class VerifiedYtDlpAsset(val version: YtDlpVersion, val size: Long, val sha256: String)

internal class UpstreamYtDlp(
    client: OkHttpClient,
    private val signingKey: () -> InputStream,
) {
    private val client = client.newBuilder()
        .followSslRedirects(false)
        .callTimeout(20, TimeUnit.SECONDS)
        .build()

    suspend fun installLatest(store: YtDlpInstallStore, probe: (File, YtDlpVersion) -> Unit): Boolean {
        val candidate = latest(store.current().version) ?: return false
        return store.install(candidate, { target -> download(candidate, target) }, probe)
    }

    private suspend fun latest(after: YtDlpVersion): VerifiedYtDlpAsset? {
        val metadata = JSONObject(readBytes(RELEASE_API, MAX_METADATA_BYTES).decodeToString())
        require(!metadata.getBoolean("draft") && !metadata.getBoolean("prerelease")) {
            "The upstream release is not stable"
        }
        val version = requireNotNull(YtDlpVersion.parse(metadata.getString("tag_name"))) {
            "The upstream yt-dlp release has an invalid version"
        }
        if (version <= after) return null
        require(metadata.getBoolean("immutable")) { "The upstream yt-dlp release is mutable" }

        val assets = metadata.getJSONArray("assets")
        val size = expectedAssetSize(assets, version, "yt-dlp", MAX_YT_DLP_BYTES)
        val checksumsSize = expectedAssetSize(assets, version, "SHA2-256SUMS", MAX_PROOF_BYTES)
        val signatureSize = expectedAssetSize(assets, version, "SHA2-256SUMS.sig", MAX_PROOF_BYTES)
        val checksums = readBytes(releaseUrl(version, "SHA2-256SUMS"), checksumsSize, checksumsSize)
        val signature = readBytes(releaseUrl(version, "SHA2-256SUMS.sig"), signatureSize, signatureSize)
        val digest = signingKey().use { verifyYtDlpChecksum(it, checksums, signature) }
        return VerifiedYtDlpAsset(version, size, digest)
    }

    private suspend fun download(asset: VerifiedYtDlpAsset, target: File) {
        val digest = MessageDigest.getInstance("SHA-256")
        FileOutputStream(target).use { output ->
            transfer(releaseUrl(asset.version, "yt-dlp"), asset.size, asset.size, output, digest)
            output.fd.sync()
        }
        check(digest.hex() == asset.sha256) { "The yt-dlp download failed its signed SHA-256 verification" }
    }

    private suspend fun readBytes(url: HttpUrl, limit: Long, exact: Long? = null): ByteArray =
        ByteArrayOutputStream().also { transfer(url, limit, exact, it) }.toByteArray()

    private suspend fun transfer(
        url: HttpUrl,
        limit: Long,
        exact: Long?,
        output: OutputStream,
        digest: MessageDigest? = null,
    ) {
        val call = client.newCall(Request.Builder().url(url).header("User-Agent", "ScreenGoatedToolbox").build())
        call.execute().use { response ->
            check(response.isSuccessful) { "Upstream yt-dlp HTTP ${response.code}" }
            val body = response.body
            val declared = body.contentLength()
            check(declared < 0 || declared <= limit) { "The upstream yt-dlp response is too large" }
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            var written = 0L
            body.byteStream().use { input ->
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val count = input.read(buffer)
                    if (count < 0) break
                    written += count
                    check(written <= limit) { "The upstream yt-dlp response exceeded its size limit" }
                    output.write(buffer, 0, count)
                    digest?.update(buffer, 0, count)
                }
            }
            check(exact == null || written == exact) { "The upstream yt-dlp response is incomplete" }
        }
    }

    private fun expectedAssetSize(
        assets: org.json.JSONArray,
        version: YtDlpVersion,
        name: String,
        limit: Long,
    ): Long {
        val matching = (0 until assets.length()).map(assets::getJSONObject)
            .singleOrNull { it.getString("name") == name }
        requireNotNull(matching) { "The upstream release is missing $name" }
        require(matching.getString("browser_download_url") == releaseUrl(version, name).toString())
        return matching.getLong("size").also { require(it in 1..limit) }
    }
}

internal fun verifyYtDlpChecksum(key: InputStream, checksums: ByteArray, detachedSignature: ByteArray): String {
    val fingerprint = BcKeyFingerprintCalculator()
    val keys = PGPPublicKeyRingCollection(PGPUtil.getDecoderStream(key), fingerprint)
    val signature = PGPUtil.getDecoderStream(detachedSignature.inputStream()).use { decoded ->
        val packets = PGPObjectFactory(decoded, fingerprint)
        val signatures = packets.nextObject() as? PGPSignatureList
        require(signatures?.size() == 1 && packets.nextObject() == null) {
            "The yt-dlp release must contain one detached checksum signature"
        }
        requireNotNull(signatures)[0]
    }
    val signer = requireNotNull(keys.getPublicKey(signature.keyID)) { "Unknown yt-dlp release signing key" }
    signature.init(BcPGPContentVerifierBuilderProvider(), signer)
    signature.update(checksums)
    check(signature.verify()) { "The yt-dlp release checksum signature is invalid" }
    return requireNotNull(
        checksums.decodeToString().lineSequence().mapNotNull(SUM_LINE::matchEntire).singleOrNull()?.groupValues?.get(1),
    ) { "The signed checksums are missing the yt-dlp executable" }
}

private fun releaseUrl(version: YtDlpVersion, asset: String): HttpUrl =
    "https://github.com/yt-dlp/yt-dlp/releases/download/${version.tag}/$asset".toHttpUrl()

private val RELEASE_API = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest".toHttpUrl()
private val SUM_LINE = Regex("""([a-f0-9]{64}) [ *]yt-dlp""")
private const val MAX_METADATA_BYTES = 512 * 1024L
private const val MAX_PROOF_BYTES = 16 * 1024L
internal const val MAX_YT_DLP_BYTES = 16 * 1024 * 1024L
