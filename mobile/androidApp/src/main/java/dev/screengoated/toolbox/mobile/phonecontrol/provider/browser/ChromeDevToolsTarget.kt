package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.net.URI
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

internal data class ChromeDevToolsTarget(
    val targetId: String,
    val title: String,
    val url: String,
    val webSocketPath: String,
)

internal data class ChromeDeepBinding(
    val handle: Int,
    val target: ChromeDevToolsTarget,
    val documentId: String,
    val loaderId: String?,
    val observationGeneration: Long,
)

internal data class ChromeTargetBaseline(
    val urlsByTargetId: Map<String, String>,
)

internal sealed interface LaunchedChromeTargetResolution {
    data class Exact(val target: ChromeDevToolsTarget) : LaunchedChromeTargetResolution
    data object Pending : LaunchedChromeTargetResolution
    data object Ambiguous : LaunchedChromeTargetResolution
}

internal fun resolveLaunchedChromeTarget(
    requestedUrl: URI,
    baseline: ChromeTargetBaseline,
    current: List<ChromeDevToolsTarget>,
): LaunchedChromeTargetResolution {
    val candidates = current.filter { target ->
        browserUrlsEquivalent(requestedUrl, browserHttpUri(target.url)) &&
            !browserUrlsEquivalent(
                browserHttpUri(baseline.urlsByTargetId[target.targetId].orEmpty()),
                browserHttpUri(target.url),
            )
    }
    return when (candidates.size) {
        0 -> LaunchedChromeTargetResolution.Pending
        1 -> LaunchedChromeTargetResolution.Exact(candidates.single())
        else -> LaunchedChromeTargetResolution.Ambiguous
    }
}

private fun browserUrlsEquivalent(left: URI?, right: URI?): Boolean {
    if (left == null || right == null) return false
    fun URI.portOrDefault(): Int = when {
        port >= 0 -> port
        scheme.equals("http", true) -> 80
        else -> 443
    }
    fun URI.pathOrRoot(): String = rawPath?.takeIf(String::isNotEmpty) ?: "/"
    return left.scheme.equals(right.scheme, true) &&
        left.host.equals(right.host, true) &&
        left.portOrDefault() == right.portOrDefault() &&
        left.pathOrRoot() == right.pathOrRoot() &&
        left.rawQuery == right.rawQuery &&
        left.rawFragment == right.rawFragment
}

internal fun parseChromeTargets(element: kotlinx.serialization.json.JsonElement): List<ChromeDevToolsTarget> {
    val targets = element as? JsonArray ?: return emptyList()
    return targets.mapNotNull { item ->
        val value = item as? JsonObject ?: return@mapNotNull null
        if (value["type"]?.jsonPrimitive?.contentOrNull != "page") return@mapNotNull null
        val targetId = value["id"]?.jsonPrimitive?.contentOrNull
            ?.takeIf(::validChromeTargetId)
            ?: return@mapNotNull null
        val rawSocket = value["webSocketDebuggerUrl"]?.jsonPrimitive?.contentOrNull
            ?: return@mapNotNull null
        val path = chromeWebSocketPath(rawSocket) ?: return@mapNotNull null
        ChromeDevToolsTarget(
            targetId = targetId,
            title = value["title"]?.jsonPrimitive?.contentOrNull.orEmpty().take(MAX_TITLE_CHARS),
            url = value["url"]?.jsonPrimitive?.contentOrNull.orEmpty().take(MAX_URL_CHARS),
            webSocketPath = path,
        )
    }.distinctBy(ChromeDevToolsTarget::targetId)
}

internal fun chromeWebSocketPath(raw: String): String? {
    val uri = runCatching { URI(raw) }.getOrNull() ?: return null
    if (uri.scheme !in setOf("ws", "wss")) return null
    val path = uri.rawPath ?: return null
    if (!path.startsWith("/devtools/") || path.contains('\\') || path.contains('\u0000')) {
        return null
    }
    if (!uri.rawQuery.isNullOrBlank() || !uri.rawFragment.isNullOrBlank()) return null
    return path
}

internal fun validChromeTargetId(value: String): Boolean =
    value.isNotBlank() &&
        value.length <= MAX_TARGET_ID_CHARS &&
        value.all { it.isLetterOrDigit() || it == '-' || it == '_' }

private const val MAX_TARGET_ID_CHARS = 256
private const val MAX_TITLE_CHARS = 512
private const val MAX_URL_CHARS = 4_096
