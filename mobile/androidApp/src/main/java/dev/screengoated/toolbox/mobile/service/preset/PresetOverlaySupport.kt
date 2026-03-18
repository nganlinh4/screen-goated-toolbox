package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import org.json.JSONObject

internal fun isExternalNavigationUrl(url: String?): Boolean {
    if (url.isNullOrBlank()) return false
    return url.startsWith("http://") || url.startsWith("https://")
}

internal fun isInternalResultUrl(url: String?): Boolean {
    if (url.isNullOrBlank()) return false
    return url.startsWith("file:///android_asset/preset_overlay/") || url.startsWith("about:")
}

internal fun String.jsonOrNull(): JSONObject? = runCatching { JSONObject(this) }.getOrNull()

internal fun Int.divCeil(divisor: Int): Int = (this + divisor - 1) / divisor

internal data class PresetCanvasWindowLayout(
    val bounds: OverlayBounds,
    val vertical: Boolean,
)

internal fun String.toResultWindowIdOrNull(): PresetResultWindowId? {
    val sessionId = substringBeforeLast(':', "")
    val blockIdx = substringAfterLast(':', "").toIntOrNull() ?: return null
    if (sessionId.isEmpty()) return null
    return PresetResultWindowId(sessionId = sessionId, blockIdx = blockIdx)
}

internal fun escapeHtml(value: String): String {
    return value
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}

internal fun loadingHtml(statusText: String): String {
    return """
        <div class="sgt-loading-shell">
            <div class="sgt-loading-indicator" aria-hidden="true"></div>
            <div class="sgt-loading-label">${escapeHtml(statusText)}</div>
        </div>
    """.trimIndent()
}

internal fun errorHtml(error: String): String {
    return "<p>${escapeHtml(error)}</p>"
}

internal fun PresetOverlayWindowSpec.asBounds(): OverlayBounds {
    return OverlayBounds(x = x, y = y, width = width, height = height)
}
