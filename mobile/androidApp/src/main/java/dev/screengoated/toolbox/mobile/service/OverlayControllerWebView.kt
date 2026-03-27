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
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.model.RealtimePaneFontSizes
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
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
    val uiPreferences: MobileUiPreferences,
)

internal data class OverlayPaneRuntimeSettings(
    val audioSource: String,
    val targetLanguage: String,
    val targetLanguageCode: String,
    val translationModel: String,
    val transcriptionModel: String,
    val fontSize: Int,
    val isDark: Boolean,
    val localeJson: String,
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
        webChromeClient = object : WebChromeClient() {
            override fun onJsAlert(
                view: WebView?,
                url: String?,
                message: String?,
                result: android.webkit.JsResult?,
            ): Boolean {
                result?.cancel()
                return true
            }
        }
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
    private var lastSettings: OverlayPaneRuntimeSettings? = null
    private var lastOldText: String = ""
    private var lastNewText: String = ""
    private var pendingInitialState: (() -> Unit)? = null

    fun render(
        html: String,
        settings: OverlayPaneRuntimeSettings,
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
        val previousSettings = lastSettings
        val settingsChanged = previousSettings != settings
        val textChanged = lastOldText != oldText || lastNewText != newText
        if (!loaded || lastHtml != html) {
            lastHtml = html
            loaded = true
            lastSettings = settings
            lastOldText = oldText
            lastNewText = newText
            Log.d(PERF_TAG, "reload-html pane=$paneId oldLen=${oldText.length} newLen=${newText.length}")
            pendingInitialState = {
                applyState(
                    previousSettings = null,
                    settings = settings,
                    oldText = oldText,
                    newText = newText,
                    textChanged = true,
                )
            }
            webView.loadDataWithBaseURL(
                "file:///android_asset/realtime_overlay/",
                html,
                "text/html",
                "utf-8",
                null,
            )
            return true
        }
        if (!settingsChanged && !textChanged) {
            return false
        }
        if (settingsChanged) {
            lastSettings = settings
        }
        if (textChanged) {
            lastOldText = oldText
            lastNewText = newText
        }
        applyState(
            previousSettings = previousSettings,
            settings = settings,
            oldText = oldText,
            newText = newText,
            textChanged = textChanged,
        )
        return false
    }

    fun onReady() {
        pendingInitialState?.invoke()
        pendingInitialState = null
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
        previousSettings: OverlayPaneRuntimeSettings?,
        settings: OverlayPaneRuntimeSettings,
        oldText: String,
        newText: String,
        textChanged: Boolean,
    ) {
        val script = buildString {
            if (previousSettings?.audioSource != settings.audioSource) {
                append("if(window.setAudioSource) window.setAudioSource(")
                append(JSONObject.quote(settings.audioSource))
                append(");")
            }
            if (previousSettings?.localeJson != settings.localeJson) {
                append("if(window.setLocaleStrings) window.setLocaleStrings(")
                append(settings.localeJson)
                append(");")
            }
            if (
                previousSettings?.targetLanguage != settings.targetLanguage ||
                previousSettings?.targetLanguageCode != settings.targetLanguageCode
            ) {
                append("if(window.setTargetLanguage) window.setTargetLanguage(")
                append(JSONObject.quote(settings.targetLanguage))
                append(", ")
                append(JSONObject.quote(settings.targetLanguageCode))
                append(");")
            }
            if (previousSettings?.translationModel != settings.translationModel) {
                append("if(window.setTranslationModel) window.setTranslationModel(")
                append(JSONObject.quote(settings.translationModel))
                append(");")
            }
            if (previousSettings?.transcriptionModel != settings.transcriptionModel) {
                append("if(window.setTranscriptionModel) window.setTranscriptionModel(")
                append(JSONObject.quote(settings.transcriptionModel))
                append(");")
            }
            if (previousSettings?.fontSize != settings.fontSize) {
                append("if(window.setFontSize) window.setFontSize(")
                append(settings.fontSize)
                append(");")
            }
            if (previousSettings?.isDark != settings.isDark) {
                append("if(window.setTheme) window.setTheme(")
                append(if (settings.isDark) "true" else "false")
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

internal fun overlayPaneRuntimeSettings(
    state: LiveSessionState,
    fontSize: Int,
    isDark: Boolean,
    uiLanguage: String,
): OverlayPaneRuntimeSettings {
    return OverlayPaneRuntimeSettings(
        audioSource = if (state.config.sourceMode == dev.screengoated.toolbox.mobile.shared.live.SourceMode.DEVICE) {
            "device"
        } else {
            "mic"
        },
        targetLanguage = state.config.targetLanguage,
        targetLanguageCode = LanguageCatalog.codeForName(state.config.targetLanguage),
        translationModel = state.config.translationProvider.id,
        transcriptionModel = state.config.transcriptionProvider.id,
        fontSize = fontSize,
        isDark = isDark,
        localeJson = overlayLocaleJson(uiLanguage),
    )
}

private fun overlayLocaleJson(uiLanguage: String): String {
    val overlay = MobileLocaleText.forLanguage(uiLanguage).overlay
    return JSONObject().apply {
        put("placeholderText", overlay.placeholderText)
        put("copyTextTitle", overlay.copyTextTitle)
        put("decreaseFontTitle", overlay.decreaseFontTitle)
        put("increaseFontTitle", overlay.increaseFontTitle)
        put("toggleTranscriptionTitle", overlay.toggleTranscriptionTitle)
        put("toggleTranslationTitle", overlay.toggleTranslationTitle)
        put("toggleHeaderTitle", overlay.toggleHeaderTitle)
        put("micInputTitle", overlay.micInputTitle)
        put("deviceAudioTitle", overlay.deviceAudioTitle)
        put("geminiLiveTitle", overlay.geminiLiveTitle)
        put("parakeetTitle", overlay.parakeetTitle)
        put("gemmaTitle", overlay.gemmaTitle)
        put("taalasTitle", overlay.taalasTitle)
        put("gtxTitle", overlay.gtxTitle)
        put("targetLanguageTitle", overlay.targetLanguageTitle)
        put("ttsSettingsTitle", overlay.ttsSettingsTitle)
        put("ttsTitle", overlay.ttsTitle)
        put("ttsSpeed", overlay.ttsSpeed)
        put("ttsAuto", overlay.ttsAuto)
        put("ttsVolume", overlay.ttsVolume)
        put("downloadingModelTitle", overlay.downloadingModelTitle)
        put("pleaseWaitText", overlay.pleaseWaitText)
        put("cancelText", overlay.cancelText)
        put("parakeetNote", overlay.parakeetNote)
    }.toString()
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
