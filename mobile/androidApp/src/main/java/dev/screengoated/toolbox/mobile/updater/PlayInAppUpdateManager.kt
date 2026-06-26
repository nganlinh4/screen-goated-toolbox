package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.IntentSenderRequest
import com.google.android.play.core.appupdate.AppUpdateInfo
import com.google.android.play.core.appupdate.AppUpdateManagerFactory
import com.google.android.play.core.appupdate.AppUpdateOptions
import com.google.android.play.core.install.InstallStateUpdatedListener
import com.google.android.play.core.install.model.AppUpdateType
import com.google.android.play.core.install.model.InstallStatus
import com.google.android.play.core.install.model.UpdateAvailability
import dev.screengoated.toolbox.mobile.BuildConfig
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Update source for the `play` flavor: drives the Google Play In-App Updates flow
 * instead of polling GitHub. The Windows desktop build remains GitHub-driven
 * (canonical), so this is the documented Android-only divergence — see
 * `.claude/parity/app-update.md`.
 *
 * The flexible flow is used: tapping "Update" launches Play's own download UI, the
 * download progresses in the background (tracked by [InstallStateUpdatedListener]),
 * and once downloaded the user taps "Restart to Update" to apply it.
 */
class PlayInAppUpdateManager(
    context: Context,
    currentVersionName: String = BuildConfig.CANONICAL_APP_VERSION,
) : AppUpdateController {
    private val appUpdateManager = AppUpdateManagerFactory.create(context.applicationContext)
    private val currentVersion = canonicalAppVersion(currentVersionName)
    private var autoCheckStarted = false
    private var cachedInfo: AppUpdateInfo? = null

    private val mutableState = MutableStateFlow(AppUpdateUiState(currentVersion = currentVersion))
    override val state: StateFlow<AppUpdateUiState> = mutableState.asStateFlow()

    private val installListener = InstallStateUpdatedListener { installState ->
        when (installState.installStatus()) {
            InstallStatus.DOWNLOADING ->
                mutableState.update { it.copy(status = AppUpdateStatus.DOWNLOADING, errorMessage = null) }
            InstallStatus.DOWNLOADED ->
                mutableState.update { it.copy(status = AppUpdateStatus.DOWNLOADED, errorMessage = null) }
            InstallStatus.FAILED ->
                mutableState.update {
                    it.copy(status = AppUpdateStatus.ERROR, errorMessage = "Play update failed")
                }
            else -> Unit
        }
    }

    init {
        // App-scoped manager (lives as long as the process), so the listener is never
        // explicitly unregistered.
        appUpdateManager.registerListener(installListener)
    }

    override fun autoCheckForUpdates() {
        if (autoCheckStarted) {
            return
        }
        autoCheckStarted = true
        checkForUpdates()
    }

    override fun checkForUpdates() {
        if (mutableState.value.status == AppUpdateStatus.CHECKING) {
            return
        }
        mutableState.update { it.copy(status = AppUpdateStatus.CHECKING, errorMessage = null) }
        appUpdateManager.appUpdateInfo
            .addOnSuccessListener { info ->
                cachedInfo = info
                when {
                    info.installStatus() == InstallStatus.DOWNLOADED -> {
                        mutableState.update {
                            it.copy(status = AppUpdateStatus.DOWNLOADED, errorMessage = null)
                        }
                    }

                    info.updateAvailability() == UpdateAvailability.UPDATE_AVAILABLE &&
                        info.isUpdateTypeAllowed(AppUpdateType.FLEXIBLE) -> {
                        mutableState.update {
                            it.copy(
                                status = AppUpdateStatus.UPDATE_AVAILABLE,
                                latestVersion = info.availableVersionCode().toString(),
                                releaseNotes = "",
                                errorMessage = null,
                            )
                        }
                    }

                    info.updateAvailability() ==
                        UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS -> {
                        mutableState.update { it.copy(status = AppUpdateStatus.DOWNLOADING) }
                    }

                    else -> {
                        mutableState.update {
                            it.copy(
                                status = AppUpdateStatus.UP_TO_DATE,
                                latestVersion = currentVersion,
                                errorMessage = null,
                            )
                        }
                    }
                }
            }
            .addOnFailureListener { error ->
                mutableState.update {
                    it.copy(
                        status = AppUpdateStatus.ERROR,
                        errorMessage = error.message ?: "Play update check failed",
                    )
                }
            }
    }

    /** Launch Play's flexible-update flow for the cached available update. */
    fun startFlexibleUpdate(launcher: ActivityResultLauncher<IntentSenderRequest>) {
        val info = cachedInfo ?: return
        runCatching {
            appUpdateManager.startUpdateFlowForResult(
                info,
                launcher,
                AppUpdateOptions.newBuilder(AppUpdateType.FLEXIBLE).build(),
            )
        }
    }

    /** Apply a downloaded flexible update (restarts the app). */
    fun completeUpdate() {
        appUpdateManager.completeUpdate()
    }
}
