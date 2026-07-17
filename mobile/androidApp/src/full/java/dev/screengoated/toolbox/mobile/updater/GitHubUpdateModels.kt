package dev.screengoated.toolbox.mobile.updater

internal data class GitHubReleaseInfo(
    val version: String,
    val body: String,
    val releaseUrl: String,
    val assetUrl: String?,
)

internal fun selectAndroidAssetUrl(assets: List<Pair<String, String>>): String? =
    assets.firstOrNull { it.first.endsWith(".apk", ignoreCase = true) }?.second
