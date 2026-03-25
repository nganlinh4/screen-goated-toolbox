package dev.screengoated.toolbox.mobile.updater

enum class AppUpdateStatus {
    IDLE,
    CHECKING,
    UP_TO_DATE,
    UPDATE_AVAILABLE,
    ERROR,
}

data class AppUpdateUiState(
    val status: AppUpdateStatus = AppUpdateStatus.IDLE,
    val currentVersion: String,
    val latestVersion: String? = null,
    val releaseNotes: String = "",
    val releaseUrl: String? = null,
    val assetUrl: String? = null,
    val errorMessage: String? = null,
    val notificationSerial: Int = 0,
) {
    val actionUrl: String?
        get() = assetUrl ?: releaseUrl
}

internal data class GitHubReleaseInfo(
    val version: String,
    val body: String,
    val releaseUrl: String,
    val assetUrl: String?,
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

internal fun selectAndroidAssetUrl(assets: List<Pair<String, String>>): String? {
    return assets.firstOrNull { it.first.endsWith(".apk", ignoreCase = true) }?.second
}

private fun versionTokens(version: String): List<Int> {
    return version
        .trim()
        .trimStart('v', 'V')
        .split(Regex("[^0-9]+"))
        .filter { it.isNotBlank() }
        .mapNotNull { it.toIntOrNull() }
}
