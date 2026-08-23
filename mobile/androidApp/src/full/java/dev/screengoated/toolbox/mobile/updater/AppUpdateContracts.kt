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
    val root = JSONObject(payload.toString(Charsets.UTF_8))
    require(root.getInt("schemaVersion") == 1 && root.getString("channel") == "stable") {
        "App update manifest contract is unsupported"
    }
    val version = strictStableVersion(root.getString("version"))
    return candidateFromAsset(
        version = version,
        body = root.optString("releaseNotes"),
        asset = root.getJSONObject("androidFullApk"),
        nameField = "name",
        urlField = "url",
        sizeField = "sizeBytes",
        digestField = "sha256",
        digestPrefix = "",
    )
}

internal fun githubCandidates(payload: ByteArray): List<AndroidUpdateCandidate> {
    require(payload.size <= MAXIMUM_GITHUB_RESPONSE_BYTES)
    val releases = JSONArray(payload.toString(Charsets.UTF_8))
    return buildList {
        for (index in 0 until releases.length()) {
            githubCandidate(releases.optJSONObject(index) ?: continue)?.let(::add)
        }
    }
}

private fun githubCandidate(release: JSONObject): AndroidUpdateCandidate? = runCatching {
    require(!release.optBoolean("draft") && !release.optBoolean("prerelease"))
    val tag = release.getString("tag_name")
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
        body = release.optString("body"),
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
    val name = asset.getString(nameField)
    require(name == expectedApkName(version)) { "Full APK name does not match its version" }
    val url = asset.getString(urlField)
    require(url == expectedApkUrl(version)) { "Full APK URL is outside the stable release contract" }
    val size = asset.getLong(sizeField)
    require(size in 1..MAXIMUM_APK_BYTES) { "Full APK size is outside the accepted range" }
    val sha256 = asset.getString(digestField).removePrefix(digestPrefix)
    require(sha256.length == 64 && sha256.all { it in '0'..'9' || it in 'a'..'f' }) {
        "Full APK SHA-256 is invalid"
    }
    return AndroidUpdateCandidate(version, body, name, url, size, sha256)
}

private fun strictStableVersion(value: String): String {
    require(STABLE_VERSION.matches(value)) { "Stable app version is invalid" }
    require(value.split('.').all { BigInteger(it) <= MAXIMUM_SEMVER_COMPONENT }) {
        "Stable app version component is too large"
    }
    return value
}

private fun expectedApkName(version: String) = "ScreenGoatedToolbox_v$version.apk"

private fun expectedApkUrl(version: String) =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v$version/${expectedApkName(version)}"

internal const val MAXIMUM_MANIFEST_BYTES = 64 * 1024
internal const val MAXIMUM_GITHUB_RESPONSE_BYTES = 16 * 1024 * 1024
private const val MAXIMUM_APK_BYTES = 2L * 1024L * 1024L * 1024L
private val MAXIMUM_SEMVER_COMPONENT = BigInteger("18446744073709551615")
private val STABLE_VERSION = Regex("^(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)$")
