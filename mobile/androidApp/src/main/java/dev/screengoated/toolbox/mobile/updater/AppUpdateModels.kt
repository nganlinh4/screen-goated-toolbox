package dev.screengoated.toolbox.mobile.updater

import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.IntentSenderRequest
import kotlinx.coroutines.flow.StateFlow

enum class AppUpdateStatus {
    IDLE,
    CHECKING,
    UP_TO_DATE,
    UPDATE_AVAILABLE,
    // Play In-App Update (play flavor) flexible-download states.
    DOWNLOADING,
    DOWNLOADED,
    ERROR,
}

/** Distribution-owned update source abstraction. */
interface AppUpdateController {
    val state: StateFlow<AppUpdateUiState>
    fun autoCheckForUpdates()
    fun checkForUpdates()
    fun startUpdate(launcher: ActivityResultLauncher<IntentSenderRequest>): Boolean = false
    fun completeUpdate(): Boolean = false
}

data class AppUpdateUiState(
    val status: AppUpdateStatus = AppUpdateStatus.IDLE,
    val currentVersion: String,
    val latestVersion: String? = null,
    val releaseNotes: String = "",
    val actionUrl: String? = null,
    val errorMessage: String? = null,
    val notificationSerial: Int = 0,
)

internal fun canonicalAppVersion(versionName: String): String = versionName.substringBefore('-').trim()

internal fun isRemoteVersionNewer(
    currentVersion: String,
    remoteVersion: String,
): Boolean {
    val current = versionTokens(canonicalAppVersion(currentVersion))
    val remote = versionTokens(canonicalAppVersion(remoteVersion))
    if (current.isEmpty() || remote.isEmpty()) {
        return canonicalAppVersion(remoteVersion) != canonicalAppVersion(currentVersion)
    }
    val maxSize = maxOf(current.size, remote.size)
    for (index in 0 until maxSize) {
        val localPart = current.getOrElse(index) { 0 }
        val remotePart = remote.getOrElse(index) { 0 }
        if (localPart != remotePart) {
            return remotePart > localPart
        }
    }
    return false
}

private fun versionTokens(version: String): List<Int> {
    return version
        .trim()
        .trimStart('v', 'V')
        .split(Regex("[^0-9]+"))
        .filter { it.isNotBlank() }
        .mapNotNull { it.toIntOrNull() }
}
