package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import dev.screengoated.toolbox.mobile.BuildConfig
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient

class AppUpdateRepository(
    context: Context,
    httpClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    currentVersionName: String = BuildConfig.CANONICAL_APP_VERSION,
) : AppUpdateController {
    private val source = AppUpdateSource(httpClient) {
        context.assets.open("component-update/public-key.hex").bufferedReader().use { reader ->
            decodeAppUpdatePublicKey(reader.readText().trim())
        }
    }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val currentVersion = canonicalAppVersion(currentVersionName)
    private val owner=AppUpdateStateOwner<Unit>(currentVersion)
    override val state: StateFlow<AppUpdateUiState> = owner.state

    override fun autoCheckForUpdates() {
        owner.beginAutoCheck()?.let(::runCheck)
    }

    override fun checkForUpdates() {
        owner.beginCheck()?.let(::runCheck)
    }

    override fun consumeNotification(notice: AppUpdateNotice?): String? = owner.consumeNotification(notice)

    private fun runCheck(check: AppUpdateCheck) {
        scope.launch {
            withContext(ioDispatcher) { source.fetchLatestRelease() }.fold(
                onSuccess = { publishCandidate(check,it) },
                onFailure = { error ->
                    owner.finishCheck(check,AppUpdateProjection(
                            status = AppUpdateStatus.ERROR,
                            errorMessage = error.message ?: "Unknown update error",
                        ))
                },
            )
        }
    }

    private fun publishCandidate(check: AppUpdateCheck, candidate: AndroidUpdateCandidate) {
        if (isRemoteVersionNewer(currentVersion, candidate.version)) {
            owner.finishCheck(check,AppUpdateProjection(
                    status = AppUpdateStatus.UPDATE_AVAILABLE,
                    latestVersion = candidate.version,
                    releaseNotes = candidate.body,
                    actionUrl = candidate.assetUrl,
                    errorMessage = null,
                    notificationVersion = candidate.version,
                ))
        } else {
            owner.finishCheck(check,AppUpdateProjection(
                    status = AppUpdateStatus.UP_TO_DATE,
                    latestVersion = currentVersion,
                    releaseNotes = "",
                    actionUrl = candidate.assetUrl,
                    errorMessage = null,
                ))
        }
    }

}
