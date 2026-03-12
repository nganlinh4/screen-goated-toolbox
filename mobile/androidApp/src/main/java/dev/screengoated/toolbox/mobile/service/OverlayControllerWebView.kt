package dev.screengoated.toolbox.mobile.service

import android.annotation.SuppressLint
import android.content.Context
import android.os.SystemClock
import android.util.Log
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.model.RealtimePaneFontSizes
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import org.json.JSONObject

internal enum class OverlayPaneId {
    TRANSCRIPTION,
    TRANSLATION,
}

internal data class OverlayBounds(
    val x: Int,
    val y: Int,
    val width: Int,
    val height: Int,
)

internal data class OverlaySnapshot(
    val state: LiveSessionState,
    val fontSizes: RealtimePaneFontSizes,
    val ttsSettings: RealtimeTtsSettings,
)

@SuppressLint("SetJavaScriptEnabled")
internal fun buildOverlayWebView(
    context: Context,
    paneId: OverlayPaneId,
    onMessage: (OverlayPaneId, String) -> Unit,
): WebView {
    return WebView(context).apply {
        overScrollMode = WebView.OVER_SCROLL_NEVER
        setBackgroundColor(android.graphics.Color.TRANSPARENT)
        isVerticalScrollBarEnabled = false
        isHorizontalScrollBarEnabled = false
        isFocusable = false
        isFocusableInTouchMode = false
        settings.javaScriptEnabled = true
        settings.domStorageEnabled = true
        settings.allowFileAccess = true
        settings.mediaPlaybackRequiresUserGesture = false
        settings.builtInZoomControls = false
        settings.displayZoomControls = false
        settings.setSupportZoom(false)
        setLayerType(View.LAYER_TYPE_HARDWARE, null)
        webChromeClient = WebChromeClient()
        addJavascriptInterface(
            object {
                @JavascriptInterface
                fun postMessage(message: String) {
                    post { onMessage(paneId, message) }
                }
            },
            "sgtAndroid",
        )
    }
}

internal class OverlayPaneHolder(
    val paneId: OverlayPaneId,
    val host: FrameLayout,
    val webView: WebView,
) {
    private var loaded = false
    private var destroyed = false
    private var lastHtml: String? = null
    private var lastRenderAtMs: Long = 0L
    private var lastSettingsJson: String? = null
    private var lastOldText: String = ""
    private var lastNewText: String = ""

    fun render(
        html: String,
        settings: JSONObject,
        oldText: String,
        newText: String,
    ): Boolean {
        val now = SystemClock.elapsedRealtime()
        val deltaMs = if (lastRenderAtMs == 0L) -1L else now - lastRenderAtMs
        if (deltaMs in 0..20) {
            Log.d(
                PERF_TAG,
                "render-burst pane=$paneId deltaMs=$deltaMs oldLen=${oldText.length} newLen=${newText.length} htmlReload=${lastHtml != html}",
            )
        }
        lastRenderAtMs = now
        val settingsJson = settings.toString()
        val settingsChanged = lastSettingsJson != settingsJson
        val textChanged = lastOldText != oldText || lastNewText != newText
        if (!loaded || lastHtml != html) {
            lastHtml = html
            loaded = true
            lastSettingsJson = settingsJson
            lastOldText = oldText
            lastNewText = newText
            Log.d(PERF_TAG, "reload-html pane=$paneId oldLen=${oldText.length} newLen=${newText.length}")
            webView.loadDataWithBaseURL(
                "file:///android_asset/realtime_overlay/",
                html,
                "text/html",
                "utf-8",
                null,
            )
            webView.postDelayed(
                {
                    applyState(
                        settingsJson = settingsJson,
                        settingsChanged = true,
                        oldText = oldText,
                        newText = newText,
                        textChanged = true,
                    )
                },
                80,
            )
            return true
        }
        if (!settingsChanged && !textChanged) {
            return false
        }
        if (settingsChanged) {
            lastSettingsJson = settingsJson
        }
        if (textChanged) {
            lastOldText = oldText
            lastNewText = newText
        }
        applyState(
            settingsJson = settingsJson,
            settingsChanged = settingsChanged,
            oldText = oldText,
            newText = newText,
            textChanged = textChanged,
        )
        return false
    }

    fun evaluate(script: String) {
        if (destroyed) {
            return
        }
        webView.post {
            if (!destroyed) {
                webView.evaluateJavascript(script, null)
            }
        }
    }

    fun destroy() {
        destroyed = true
        webView.removeJavascriptInterface("sgtAndroid")
        webView.stopLoading()
        webView.destroy()
    }

    private fun applyState(
        settingsJson: String,
        settingsChanged: Boolean,
        oldText: String,
        newText: String,
        textChanged: Boolean,
    ) {
        val script = buildString {
            if (settingsChanged) {
                append("if(window.updateSettings) window.updateSettings(")
                append(settingsJson)
                append(");")
            }
            if (textChanged) {
                append("if(window.updateText) window.updateText(")
                append(JSONObject.quote(oldText))
                append(", ")
                append(JSONObject.quote(newText))
                append(");")
            }
        }
        if (script.isNotEmpty()) {
            evaluate(script)
        }
    }

    private companion object {
        private const val PERF_TAG = "SGTOverlayPerf"
    }
}

internal fun overlayPaneSettingsJson(
    state: LiveSessionState,
    fontSize: Int,
): JSONObject {
    return JSONObject()
        .put("audioSource", if (state.config.sourceMode == dev.screengoated.toolbox.mobile.shared.live.SourceMode.DEVICE) "device" else "mic")
        .put("targetLanguage", state.config.targetLanguage)
        .put("targetLanguageCode", LanguageCatalog.codeForName(state.config.targetLanguage))
        .put("translationModel", state.config.translationProvider.id)
        .put("transcriptionModel", state.config.transcriptionProvider.id)
        .put("fontSize", fontSize)
}

internal fun transcriptOldText(state: LiveSessionState): String {
    val transcript = state.liveText.fullTranscript
    val committed = state.liveText.lastCommittedPos.coerceIn(0, transcript.length)
    return transcript.substring(0, committed).trimEnd()
}

internal fun transcriptNewText(state: LiveSessionState): String {
    val transcript = state.liveText.fullTranscript
    val committed = state.liveText.lastCommittedPos.coerceIn(0, transcript.length)
    val rawNew = transcript.substring(committed).trimStart()
    return if (rawNew.isNotEmpty() && transcriptOldText(state).isNotEmpty()) {
        " $rawNew"
    } else {
        rawNew
    }
}
