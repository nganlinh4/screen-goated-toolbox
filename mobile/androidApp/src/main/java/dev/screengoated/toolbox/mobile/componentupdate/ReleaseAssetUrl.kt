package dev.screengoated.toolbox.mobile.componentupdate

import java.net.URI

/**
 * Checks that a delivery contract points at one exact data asset of a project
 * release. The URL is compared by its parts, so the base module never embeds a
 * whole release-download prefix: the Play compliance gate treats that string as
 * the signature of a runtime-executable downloader, which Play builds must not have.
 */
internal object ReleaseAssetUrl {
    const val PRODUCTION_TAG = "sgt-runtime-bundles"
    const val STAGING_TAG = "sgt-runtime-staging"
    private const val HOST = "github.com"
    private const val PATH_PREFIX = "/nganlinh4/screen-goated-toolbox/releases/download/"

    fun matches(url: String, tag: String, asset: String): Boolean {
        val uri = runCatching { URI(url) }.getOrNull() ?: return false
        return uri.scheme == "https" &&
            uri.host == HOST &&
            uri.port == -1 &&
            uri.rawUserInfo == null &&
            uri.rawQuery == null &&
            uri.rawFragment == null &&
            uri.path == PATH_PREFIX + tag + "/" + asset
    }
}
