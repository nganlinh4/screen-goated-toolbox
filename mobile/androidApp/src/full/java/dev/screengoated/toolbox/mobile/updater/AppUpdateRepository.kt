package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import dev.screengoated.toolbox.mobile.BuildConfig
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import okio.Buffer
import java.math.BigInteger
import java.util.concurrent.TimeUnit

class AppUpdateRepository(
    private val context: Context,
    private val httpClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    currentVersionName: String = BuildConfig.CANONICAL_APP_VERSION,
) : AppUpdateController {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val currentVersion = canonicalAppVersion(currentVersionName)
    private var autoCheckStarted = false
    private val mutableState = MutableStateFlow(AppUpdateUiState(currentVersion = currentVersion))
    override val state: StateFlow<AppUpdateUiState> = mutableState.asStateFlow()

    override fun autoCheckForUpdates() {
        if (autoCheckStarted) return
        autoCheckStarted = true
        checkForUpdates()
    }

    override fun checkForUpdates() {
        if (mutableState.value.status == AppUpdateStatus.CHECKING) return
        scope.launch {
            mutableState.update { it.copy(status = AppUpdateStatus.CHECKING, errorMessage = null) }
            withContext(ioDispatcher) { fetchLatestRelease() }.fold(
                onSuccess = ::publishCandidate,
                onFailure = { error ->
                    mutableState.update {
                        it.copy(
                            status = AppUpdateStatus.ERROR,
                            errorMessage = error.message ?: "Unknown update error",
                        )
                    }
                },
            )
        }
    }

    private fun publishCandidate(candidate: AndroidUpdateCandidate) {
        if (isRemoteVersionNewer(currentVersion, candidate.version)) {
            mutableState.update {
                it.copy(
                    status = AppUpdateStatus.UPDATE_AVAILABLE,
                    latestVersion = candidate.version,
                    releaseNotes = candidate.body,
                    actionUrl = candidate.assetUrl,
                    errorMessage = null,
                    notificationSerial = it.notificationSerial + 1,
                )
            }
        } else {
            mutableState.update {
                it.copy(
                    status = AppUpdateStatus.UP_TO_DATE,
                    latestVersion = currentVersion,
                    releaseNotes = "",
                    actionUrl = candidate.assetUrl,
                    errorMessage = null,
                )
            }
        }
    }

    private fun fetchLatestRelease(): Result<AndroidUpdateCandidate> = runCatching {
        fetchStableManifest() ?: fetchGitHubFallback()
    }

    private fun fetchStableManifest(): AndroidUpdateCandidate? {
        val payload = fetchBytesOrNull(MANIFEST_URL, MAXIMUM_MANIFEST_BYTES) ?: return null
        val signature = fetchBytes(SIGNATURE_URL, 64)
        require(signature.size == 64) { "App update signature shape is invalid" }
        val publicPoint = context.assets.open(PUBLIC_KEY_ASSET).bufferedReader().use { reader ->
            decodeHex(reader.readText().trim())
        }
        return verifiedStableManifest(payload, signature, publicPoint)
    }

    private fun fetchGitHubFallback(): AndroidUpdateCandidate {
        val candidates = mutableListOf<AndroidUpdateCandidate>()
        for (page in 1..MAXIMUM_RELEASE_PAGES) {
            val payload = fetchBytes(
                "$RELEASES_URL?per_page=$RELEASES_PER_PAGE&page=$page",
                MAXIMUM_GITHUB_RESPONSE_BYTES,
            )
            candidates += githubCandidates(payload)
            if (JSONArray(payload.toString(Charsets.UTF_8)).length() < RELEASES_PER_PAGE) break
        }
        return candidates.maxWithOrNull(
            Comparator { left, right -> compareCanonicalVersions(left.version, right.version) },
        ) ?: error("No compatible stable GitHub release was found")
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

    private fun decodeHex(value: String): ByteArray {
        require(
            value.length == 130 && value.startsWith("04") &&
                value.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' },
        )
        return ByteArray(value.length / 2) { index ->
            value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
        }
    }

    private companion object {
        const val MANIFEST_URL =
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/app-update-feed/stable-v1.json"
        const val SIGNATURE_URL =
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/app-update-feed/stable-v1.sig"
        const val RELEASES_URL =
            "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases"
        const val PUBLIC_KEY_ASSET = "component-update/public-key.hex"
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
