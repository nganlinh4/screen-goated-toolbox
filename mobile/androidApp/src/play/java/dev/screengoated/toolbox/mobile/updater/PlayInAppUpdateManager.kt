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
import kotlinx.coroutines.flow.StateFlow

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
    private val owner=AppUpdateStateOwner<AppUpdateInfo>(currentVersion)
    override val state: StateFlow<AppUpdateUiState> = owner.state

    private val installListener = InstallStateUpdatedListener { installState ->
        when (installState.installStatus()) {
            InstallStatus.PENDING, InstallStatus.DOWNLOADING ->
                owner.installState(AppUpdateStatus.DOWNLOADING)
            InstallStatus.DOWNLOADED ->
                owner.installState(AppUpdateStatus.DOWNLOADED)
            InstallStatus.INSTALLING -> owner.installState(AppUpdateStatus.INSTALLING)
            InstallStatus.FAILED, InstallStatus.CANCELED ->
                owner.installState(AppUpdateStatus.ERROR,"Play update failed")
            else -> Unit
        }
    }

    init {
        // App-scoped manager (lives as long as the process), so the listener is never
        // explicitly unregistered.
        appUpdateManager.registerListener(installListener)
    }

    override fun autoCheckForUpdates() {
        owner.beginAutoCheck()?.let(::runCheck)
    }

    override fun checkForUpdates() {
        owner.beginCheck()?.let(::runCheck)
    }

    override fun consumeNotification(notice: AppUpdateNotice?): String? = owner.consumeNotification(notice)

    private fun runCheck(check: AppUpdateCheck) {
        try {
            appUpdateManager.appUpdateInfo
                .addOnSuccessListener { info ->
                    val projection=runCatching {
                        playUpdateProjection(info.installStatus(),info.updateAvailability(),
                            { info.isUpdateTypeAllowed(AppUpdateType.FLEXIBLE) },{ info.availableVersionCode() },currentVersion)
                    }.getOrElse { AppUpdateProjection(AppUpdateStatus.ERROR,errorMessage="Play update check failed") }
                    owner.finishCheck(check,projection,info)
                }
                .addOnFailureListener { error ->
                    owner.finishCheck(check,AppUpdateProjection(AppUpdateStatus.ERROR,
                        errorMessage=error.message ?: "Play update check failed"))
                }
        } catch (_: Exception) {
            owner.finishCheck(check,AppUpdateProjection(AppUpdateStatus.ERROR,errorMessage="Play update check failed"))
        }
    }

    /** Launch Play's flexible-update flow for the cached available update. */
    override fun startUpdate(launcher: ActivityResultLauncher<IntentSenderRequest>): Boolean {
        val launch = owner.claimAction() ?: return false
        val accepted = tryStartPlayUpdate {
            appUpdateManager.startUpdateFlowForResult(
                launch.value,
                launcher,
                AppUpdateOptions.newBuilder(AppUpdateType.FLEXIBLE).build(),
            )
        }
        owner.finishLaunch(launch,accepted,"Play update failed")
        return accepted
    }

    /** Apply a downloaded flexible update (restarts the app). */
    override fun completeUpdate(): Boolean {
        val completion=owner.beginCompletion() ?: return false
        return try {
            appUpdateManager.completeUpdate()
                .addOnSuccessListener { owner.finishCompletion(completion,null) }
                .addOnFailureListener { owner.finishCompletion(completion,"Play update failed") }
            true
        } catch (_: Exception) {
            owner.finishCompletion(completion,"Play update failed")
            false
        }
    }
}

internal fun playUpdateProjection(
    installStatus: Int,
    availability: Int,
    flexibleAllowed: () -> Boolean,
    availableVersion: () -> Int,
    currentVersion: String,
): AppUpdateProjection = when {
    installStatus==InstallStatus.DOWNLOADED -> AppUpdateProjection(AppUpdateStatus.DOWNLOADED)
    installStatus==InstallStatus.INSTALLING -> AppUpdateProjection(AppUpdateStatus.INSTALLING)
    installStatus in setOf(InstallStatus.PENDING,InstallStatus.DOWNLOADING) -> AppUpdateProjection(AppUpdateStatus.DOWNLOADING)
    availability==UpdateAvailability.UPDATE_AVAILABLE -> {
        if (!flexibleAllowed()) unavailablePlayUpdate() else {
            val version=availableVersion()
            if (version>0) AppUpdateProjection(AppUpdateStatus.UPDATE_AVAILABLE,latestVersion=playVersionName(version))
            else unavailablePlayUpdate()
        }
    }
    availability==UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS -> AppUpdateProjection(AppUpdateStatus.DOWNLOADING)
    availability==UpdateAvailability.UPDATE_NOT_AVAILABLE -> AppUpdateProjection(AppUpdateStatus.UP_TO_DATE,latestVersion=currentVersion)
    else -> unavailablePlayUpdate()
}

/** Inverse of build.gradle.kts semverToVersionCode: 50700 -> "5.7.0", 41001 -> "4.10.1". */
internal fun playVersionName(versionCode: Int): String =
    "${versionCode / 10000}.${versionCode / 100 % 100}.${versionCode % 100}"

private fun unavailablePlayUpdate()=AppUpdateProjection(AppUpdateStatus.ERROR,errorMessage="Play update is unavailable")
