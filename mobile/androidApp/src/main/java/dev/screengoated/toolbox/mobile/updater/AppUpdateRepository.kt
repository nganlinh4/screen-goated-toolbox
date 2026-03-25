package dev.screengoated.toolbox.mobile.updater

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

class AppUpdateRepository(
    private val httpClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    currentVersionName: String = BuildConfig.CANONICAL_APP_VERSION,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val currentVersion = canonicalAppVersion(currentVersionName)
    private var autoCheckStarted = false

    private val mutableState = MutableStateFlow(
        AppUpdateUiState(currentVersion = currentVersion),
    )
    val state: StateFlow<AppUpdateUiState> = mutableState.asStateFlow()

    fun autoCheckForUpdates() {
        if (autoCheckStarted) {
            return
        }
        autoCheckStarted = true
        checkForUpdates()
    }

    fun checkForUpdates() {
        if (mutableState.value.status == AppUpdateStatus.CHECKING) {
            return
        }
        scope.launch {
            mutableState.update {
                it.copy(
                    status = AppUpdateStatus.CHECKING,
                    errorMessage = null,
                )
            }

            val latestRelease = withContext(ioDispatcher) { fetchLatestRelease() }
            latestRelease.fold(
                onSuccess = { release ->
                    if (isRemoteVersionNewer(currentVersion, release.version)) {
                        mutableState.update {
                            it.copy(
                                status = AppUpdateStatus.UPDATE_AVAILABLE,
                                latestVersion = release.version,
                                releaseNotes = release.body,
                                releaseUrl = release.releaseUrl,
                                assetUrl = release.assetUrl,
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
                                releaseUrl = release.releaseUrl,
                                assetUrl = release.assetUrl,
                                errorMessage = null,
                            )
                        }
                    }
                },
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

    private fun fetchLatestRelease(): Result<GitHubReleaseInfo> = runCatching {
        val request = Request.Builder()
            .url(LATEST_RELEASES_URL)
            .header("User-Agent", "screen-goated-toolbox-android-updater")
            .build()

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                error("Failed to fetch release info: HTTP ${response.code}")
            }
            val rawBody = response.body?.string().orEmpty()
            val releases = JSONArray(rawBody)
            if (releases.length() == 0) {
                error("No releases found on GitHub")
            }
            val latest = releases.optJSONObject(0) ?: error("Invalid release response")
            val version = canonicalAppVersion(
                latest.optString("tag_name").removePrefix("v"),
            )
            if (version.isBlank()) {
                error("Latest release is missing a version tag")
            }
            val assetsJson = latest.optJSONArray("assets")
            val assets = buildList {
                if (assetsJson != null) {
                    for (index in 0 until assetsJson.length()) {
                        val asset = assetsJson.optJSONObject(index) ?: continue
                        val name = asset.optString("name")
                        val url = asset.optString("browser_download_url")
                        if (name.isNotBlank() && url.isNotBlank()) {
                            add(name to url)
                        }
                    }
                }
            }
            GitHubReleaseInfo(
                version = version,
                body = latest.optString("body"),
                releaseUrl = latest.optString("html_url"),
                assetUrl = selectAndroidAssetUrl(assets),
            )
        }
    }

    private companion object {
        const val LATEST_RELEASES_URL =
            "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases?per_page=1&prerelease=false"
    }
}
