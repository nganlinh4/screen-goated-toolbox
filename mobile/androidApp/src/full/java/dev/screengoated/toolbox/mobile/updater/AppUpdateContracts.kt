package dev.screengoated.toolbox.mobile.updater

import dev.screengoated.toolbox.mobile.componentupdate.verifyP256Signature
import java.math.BigInteger
import org.json.JSONArray
import org.json.JSONObject

internal data class AndroidUpdateCandidate(
    val version: String,
    val body: String,
    val assetName: String,
    val assetUrl: String,
    val sizeBytes: Long,
    val sha256: String,
)

internal fun verifiedStableManifest(
    payload: ByteArray,
    signature: ByteArray,
    publicPoint: ByteArray,
): AndroidUpdateCandidate {
    require(payload.isNotEmpty() && payload.size <= MAXIMUM_MANIFEST_BYTES)
    require(signature.size == 64 && verifyP256Signature(publicPoint, payload, signature)) {
        "App update manifest signature is invalid"
    }
    return stableManifestCandidate(payload)
}

internal fun stableManifestCandidate(payload: ByteArray): AndroidUpdateCandidate {
    require(payload.isNotEmpty() && payload.size <= MAXIMUM_MANIFEST_BYTES)
    val root = JSONObject(payload.decodeToString(throwOnInvalidSequence = true))
    require(integerField(root, "schemaVersion") == 1L && textField(root, "channel") == "stable") {
        "App update manifest contract is unsupported"
    }
    val version = strictStableVersion(textField(root, "version"))
    readAsset(root.getJSONObject("installer"), version, "exe", MAXIMUM_INSTALLER_BYTES,
        "name", "url", "sizeBytes", "sha256", "")
    return candidateFromAsset(
        version = version,
        body = optionalText(root, "releaseNotes"),
        asset = root.getJSONObject("androidFullApk"),
        nameField = "name",
        urlField = "url",
        sizeField = "sizeBytes",
        digestField = "sha256",
        digestPrefix = "",
    )
}

internal fun githubCandidates(payload: ByteArray): List<AndroidUpdateCandidate> =
    githubReleasePage(payload).candidates

internal data class GithubUpdatePage(val count: Int, val candidates: List<AndroidUpdateCandidate>)

internal fun githubReleasePage(payload: ByteArray): GithubUpdatePage {
    require(payload.size <= MAXIMUM_GITHUB_RESPONSE_BYTES)
    val releases = JSONArray(payload.decodeToString(throwOnInvalidSequence = true))
    val candidates = buildList {
        for (index in 0 until releases.length()) {
            githubCandidate(releases.optJSONObject(index) ?: continue)?.let(::add)
        }
    }
    return GithubUpdatePage(releases.length(), candidates)
}

private fun githubCandidate(release: JSONObject): AndroidUpdateCandidate? = runCatching {
    require(!booleanField(release, "draft") && !booleanField(release, "prerelease"))
    val tag = textField(release, "tag_name")
    require(tag.startsWith('v'))
    val version = strictStableVersion(tag.removePrefix("v"))
    val expectedName = expectedApkName(version)
    val assets = release.getJSONArray("assets")
    val matches = buildList {
        for (index in 0 until assets.length()) {
            assets.optJSONObject(index)
                ?.takeIf { it.optString("name") == expectedName }
                ?.let(::add)
        }
    }
    require(matches.size == 1) { "Stable release must contain one exact Full APK" }
    candidateFromAsset(
        version = version,
        body = optionalText(release, "body", allowNull = true),
        asset = matches.single(),
        nameField = "name",
        urlField = "browser_download_url",
        sizeField = "size",
        digestField = "digest",
        digestPrefix = "sha256:",
    )
}.getOrNull()

private fun candidateFromAsset(
    version: String,
    body: String,
    asset: JSONObject,
    nameField: String,
    urlField: String,
    sizeField: String,
    digestField: String,
    digestPrefix: String,
): AndroidUpdateCandidate {
    val selected = readAsset(asset, version, "apk", MAXIMUM_APK_BYTES,
        nameField, urlField, sizeField, digestField, digestPrefix)
    return AndroidUpdateCandidate(version, body, selected.name, selected.url, selected.size, selected.sha256)
}

private data class StableReleaseAsset(val name: String, val url: String, val size: Long, val sha256: String)

private fun readAsset(
    asset: JSONObject,
    version: String,
    extension: String,
    maximumBytes: Long,
    nameField: String,
    urlField: String,
    sizeField: String,
    digestField: String,
    digestPrefix: String,
): StableReleaseAsset {
    val expectedName = "ScreenGoatedToolbox_v$version.$extension"
    val name = textField(asset, nameField)
    require(name == expectedName) { "Update package name does not match its version" }
    val url = textField(asset, urlField)
    require(url == "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v$version/$expectedName") {
        "Update package URL is outside the stable release contract"
    }
    val size = integerField(asset, sizeField)
    require(size in 1..maximumBytes) { "Update package size is outside the accepted range" }
    val digest = textField(asset, digestField)
    require(digest.startsWith(digestPrefix)) { "Update package digest algorithm is invalid" }
    val sha256 = digest.removePrefix(digestPrefix)
    require(sha256.length == 64 && sha256.all { it in '0'..'9' || it in 'a'..'f' }) {
        "Update package SHA-256 is invalid"
    }
    return StableReleaseAsset(name, url, size, sha256)
}

private fun textField(root: JSONObject, name: String): String =
    root.get(name) as? String ?: error("Update field $name must be text")

private fun optionalText(root: JSONObject, name: String, allowNull: Boolean = false): String {
    if (!root.has(name) || allowNull && root.isNull(name)) return ""
    return textField(root, name)
}

private fun booleanField(root: JSONObject, name: String): Boolean =
    root.get(name) as? Boolean ?: error("Update field $name must be a boolean")

private fun integerField(root: JSONObject, name: String): Long {
    val value = root.get(name)
    require(value is Number && JSON_INTEGER.matches(value.toString())) { "Update field $name must be an integer" }
    return value.toString().toLongOrNull() ?: error("Update field $name is outside the accepted range")
}

private fun strictStableVersion(value: String): String {
    require(STABLE_VERSION.matches(value)) { "Stable app version is invalid" }
    require(value.split('.').all { BigInteger(it) <= MAXIMUM_SEMVER_COMPONENT }) {
        "Stable app version component is too large"
    }
    return value
}

private fun expectedApkName(version: String) = "ScreenGoatedToolbox_v$version.apk"

internal const val MAXIMUM_MANIFEST_BYTES = 64 * 1024
internal const val MAXIMUM_GITHUB_RESPONSE_BYTES = 16 * 1024 * 1024
private const val MAXIMUM_APK_BYTES = 2L * 1024L * 1024L * 1024L
private const val MAXIMUM_INSTALLER_BYTES = 1024L * 1024L * 1024L
private val JSON_INTEGER = Regex("^(0|[1-9][0-9]*)$")
private val MAXIMUM_SEMVER_COMPONENT = BigInteger("18446744073709551615")
private val STABLE_VERSION = Regex("^(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)$")
