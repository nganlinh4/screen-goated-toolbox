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

internal fun loadingHtml(@Suppress("UNUSED_PARAMETER") statusText: String): String {
    return """
        <div class="sgt-loading-shell">
            <canvas id="sgt-m3e-canvas" aria-hidden="true"></canvas>
        </div>
    """.trimIndent()
}

internal fun errorHtml(error: String): String {
    return "<p>${escapeHtml(error)}</p>"
}

internal fun overlayRecoveryFailureHtml(description: String): String {
    val escaped = escapeHtml(description)
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                body {
                    margin: 0;
                    min-height: 100vh;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    background: linear-gradient(180deg, #101218, #171b24);
                    color: #f5f7fb;
                    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
                }
                .card {
                    max-width: 420px;
                    margin: 20px;
                    padding: 18px 20px;
                    border-radius: 18px;
                    background: rgba(27, 31, 43, 0.96);
                    border: 1px solid rgba(255, 255, 255, 0.1);
                    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.28);
                }
                h1 {
                    margin: 0 0 8px;
                    font-size: 18px;
                    font-weight: 700;
                }
                p {
                    margin: 0;
                    line-height: 1.45;
                    color: #d7deeb;
                    font-size: 14px;
                }
                .meta {
                    margin-top: 10px;
                    font-size: 12px;
                    color: #99a6bd;
                }
            </style>
        </head>
        <body>
            <div class="card">
                <h1>Overlay recovered</h1>
                <p>The page hit a loading error, so the overlay restored a safe view instead of leaving the Android error page onscreen.</p>
                <div class="meta">$escaped</div>
            </div>
        </body>
        </html>
    """.trimIndent()
}

internal fun PresetOverlayWindowSpec.asBounds(): OverlayBounds {
    return OverlayBounds(x = x, y = y, width = width, height = height)
}
