package dev.screengoated.toolbox.mobile.service.preset

import android.util.Log
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

private val untrustedGestureMessages = setOf(
    "activateResultWindow", "promoteResultWindow", "dragResultWindow",
    "dragResultWindowAt", "dragResultWindowEnd", "resizeResultWindow",
    "resizeResultWindowEnd", "cancelResultGesture", "copySelectedText", "downloadInputAudio",
)

internal fun allowUntrustedResultGestureMessage(
    message: String,
    expectedWindowId: String,
    isMainFrame: Boolean,
): Boolean {
    if (!isMainFrame || message.length > 4_000_000) return false
    val payload = runCatching { Json.parseToJsonElement(message).jsonObject }.getOrNull() ?: return false
    val type = runCatching { payload["type"]?.jsonPrimitive?.content }.getOrNull()
    val id = runCatching { payload["windowId"]?.jsonPrimitive?.content }.getOrNull()
    return id == expectedWindowId && type in untrustedGestureMessages
}

/** Installs only into a separate WebView that has no page-world native bridge. */
internal fun installUntrustedResultGestureBridge(
    view: WebView,
    windowId: String,
    isDark: Boolean,
    onMessage: (String) -> Unit,
): Boolean {
    if (!WebViewFeature.isFeatureSupported(WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD)) return false
    return runCatching {
        val world = WebViewCompat.getExecutionWorld(view, "sgt-result-gestures")
        val origins = setOf("*")
        WebViewCompat.addWebMessageListener(view, "sgtAndroid", origins, world) { _, message, _, isMainFrame, _ ->
            val data = message.data ?: return@addWebMessageListener
            if (allowUntrustedResultGestureMessage(data, windowId, isMainFrame)) onMessage(data)
        }
        try {
            WebViewCompat.addJavaScriptOnEvent(
                view,
                "(function(){ if (window !== window.top) return; " +
                    "document.documentElement.setAttribute('data-sgt-native-audio-download','1'); " +
                    presetHostedRawPageBootstrapScript(windowId, isDark) + " })();",
                WebViewCompat.INJECTION_EVENT_DOCUMENT_END,
                origins,
                world,
            )
        } catch (error: Exception) {
            WebViewCompat.removeWebMessageListener(view, world, "sgtAndroid")
            throw error
        }
        true
    }.onFailure { Log.w("PresetOverlay", "Isolated result gestures unavailable", it) }.getOrDefault(false)
}
