package dev.screengoated.toolbox.mobile.service.overlay

import android.content.Context
import dev.screengoated.toolbox.mobile.model.LanguageCatalog

internal data class RealtimeOverlayPaneSettings(
    val isTranslation: Boolean,
    val audioSource: String,
    val targetLanguage: String,
    val translationModel: String,
    val transcriptionModel: String,
    val fontSize: Int,
    val isDark: Boolean,
)

internal class RealtimeOverlayHtmlBuilder(
    private val context: Context,
) {
    private val baseHtml by lazy { overlayBaseHtmlTemplate() }
    private val baseCss by lazy { asset("style.css") }
    private val baseMainJs by lazy { asset("main.js") }
    private val baseLogicJs by lazy { asset("logic.js") }
    private val htmlCache = LinkedHashMap<RealtimeOverlayPaneSettings, String>()

    fun build(settings: RealtimeOverlayPaneSettings): String {
        return htmlCache.getOrPut(settings) {
            val replacements = linkedMapOf(
                "FONT_CSS" to overlayFontCss(),
                "CSS_CONTENT" to overlayCss(
                    baseCss = baseCss,
                    glowColor = if (settings.isTranslation) "#ff9633" else "#00c8ff",
                    fontSize = settings.fontSize,
                    isDark = settings.isDark,
                ),
                "JS_CONTENT" to javascript(settings),
                "TITLE_CONTENT" to if (settings.isTranslation) "" else """<canvas id="volume-canvas" width="90" height="24"></canvas>""",
                "AUDIO_SELECTOR" to controls(settings),
                "LOADING_ICON" to if (settings.isTranslation) {
                    RealtimeOverlayIcons.TRANSLATION_LOADING
                } else {
                    RealtimeOverlayIcons.TRANSCRIPTION_LOADING
                },
                "PLACEHOLDER_TEXT" to OVERLAY_PLACEHOLDER_TEXT,
                "CONTENT_COPY_SVG" to RealtimeOverlayIcons.CONTENT_COPY,
                "REMOVE_SVG" to RealtimeOverlayIcons.REMOVE,
                "ADD_SVG" to RealtimeOverlayIcons.ADD,
                "SUBTITLES_SVG" to RealtimeOverlayIcons.SUBTITLES,
                "TRANSLATE_SVG" to RealtimeOverlayIcons.TRANSLATE,
                "EXPAND_LESS_SVG" to RealtimeOverlayIcons.EXPAND_LESS,
                "DOWNLOAD_SVG" to RealtimeOverlayIcons.DOWNLOAD,
                "SUPPORTS_ENGLISH" to OVERLAY_PARAKEET_NOTE,
                "CLOSE_SVG" to RealtimeOverlayIcons.CLOSE,
                "CANCEL_TEXT" to OVERLAY_CANCEL_TEXT,
                "VOLUME_UP_SVG" to RealtimeOverlayIcons.VOLUME_UP,
                "TTS_TITLE" to OVERLAY_TTS_TITLE,
                "TTS_SPEED" to OVERLAY_TTS_SPEED,
                "TTS_AUTO" to OVERLAY_TTS_AUTO,
                "TTS_VOLUME" to OVERLAY_TTS_VOLUME,
                "MIC_ACTIVE" to if (!settings.isTranslation && settings.audioSource == "mic") "active" else "",
                "DEVICE_ACTIVE" to if (!settings.isTranslation && settings.audioSource == "device") "active" else "",
                "GEMINI_ACTIVE" to if (settings.transcriptionModel == "gemini") "active" else "",
                "PARAKEET_ACTIVE" to if (settings.transcriptionModel == "parakeet") "active" else "",
                "GEMMA_ACTIVE" to if (settings.translationModel == "google-gemma") "active" else "",
                "CEREBRAS_ACTIVE" to if (settings.translationModel == "cerebras-oss") "active" else "",
                "GTX_ACTIVE" to if (settings.translationModel == "google-gtx") "active" else "",
                "MIC_SVG" to RealtimeOverlayIcons.MIC,
                "DEVICE_SVG" to RealtimeOverlayIcons.SPEAKER_GROUP,
                "AUTO_AWESOME_SVG" to RealtimeOverlayIcons.AUTO_AWESOME,
                "BOLT_EN_SVG" to RealtimeOverlayIcons.BOLT_EN,
                "SPEED_SVG" to RealtimeOverlayIcons.SPEED,
                "LANGUAGE_SVG" to RealtimeOverlayIcons.LANGUAGE,
            )
            replacements.entries.fold(baseHtml) { html, (token, value) ->
                html.replace("{{$token}}", value)
            }
        }
    }

    private fun javascript(settings: RealtimeOverlayPaneSettings): String {
        return buildString {
            append(overlayBridgePrelude())
            append('\n')
            append(
                baseMainJs
                    .replace("{{FONT_SIZE}}", settings.fontSize.toString())
                    .replace("{{CHECK_SVG}}", RealtimeOverlayIcons.CHECK)
                    .replace("{{COPY_SVG}}", RealtimeOverlayIcons.CONTENT_COPY),
            )
            append('\n')
            append(baseLogicJs.replace("{{PLACEHOLDER_TEXT}}", OVERLAY_PLACEHOLDER_TEXT))
            append('\n')
            append(overlayMobileShim())
        }
    }

    private fun controls(settings: RealtimeOverlayPaneSettings): String {
        return if (settings.isTranslation) {
            """
            <span class="ctrl-btn speak-btn" id="speak-btn" title="Text-to-Speech Settings"><span class="material-symbols-rounded">${RealtimeOverlayIcons.VOLUME_UP}</span></span>
            <div class="btn-group">
                <span class="material-symbols-rounded model-icon ${if (settings.translationModel == "google-gemma") "active" else ""}" data-value="google-gemma" title="AI Translation (Gemma)">${RealtimeOverlayIcons.AUTO_AWESOME}</span>
                <span class="material-symbols-rounded model-icon ${if (settings.translationModel == "cerebras-oss") "active" else ""}" data-value="cerebras-oss" title="Instant AI (Cerebras)">${RealtimeOverlayIcons.SPEED}</span>
                <span class="material-symbols-rounded model-icon ${if (settings.translationModel == "google-gtx") "active" else ""}" data-value="google-gtx" title="Unlimited Translation (Google)">${RealtimeOverlayIcons.LANGUAGE}</span>
            </div>
            <button class="language-btn" id="language-select" type="button" title="Target Language: ${settings.targetLanguage}" data-language="${settings.targetLanguage}" data-code="${LanguageCatalog.codeForName(settings.targetLanguage)}">
                <span id="language-select-code">${LanguageCatalog.codeForName(settings.targetLanguage)}</span>
            </button>
            """.trimIndent()
        } else {
            """
            <div class="btn-group">
                <span class="material-symbols-rounded audio-icon ${if (settings.audioSource == "mic") "active" else ""}" id="mic-btn" data-value="mic" title="Microphone Input">${RealtimeOverlayIcons.MIC}</span>
                <span class="material-symbols-rounded audio-icon ${if (settings.audioSource == "device") "active" else ""}" id="device-btn" data-value="device" title="Device Audio">${RealtimeOverlayIcons.SPEAKER_GROUP}</span>
            </div>
            <div class="btn-group">
                <span class="material-symbols-rounded trans-model-icon ${if (settings.transcriptionModel == "gemini") "active" else ""}" data-value="gemini" title="Gemini Live (Cloud)">${RealtimeOverlayIcons.AUTO_AWESOME}</span>
                <span class="material-symbols-rounded trans-model-icon ${if (settings.transcriptionModel == "parakeet") "active" else ""}" data-value="parakeet" title="Parakeet (Local)">${RealtimeOverlayIcons.BOLT_EN}</span>
            </div>
            """.trimIndent()
        }
    }

    private fun asset(name: String): String {
        return context.assets.open("realtime_overlay/$name").bufferedReader().use { it.readText() }
    }
}
