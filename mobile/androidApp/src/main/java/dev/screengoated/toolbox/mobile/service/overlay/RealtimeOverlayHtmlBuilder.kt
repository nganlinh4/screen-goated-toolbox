package dev.screengoated.toolbox.mobile.service.overlay

import android.content.Context
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal data class RealtimeOverlayPaneSettings(
    val isTranslation: Boolean,
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
            val locale = MobileLocaleText.forLanguage("en")
            val replacements = linkedMapOf(
                "FONT_CSS" to overlayFontCss(),
                "CSS_CONTENT" to overlayCss(
                    baseCss = baseCss,
                    glowColor = if (settings.isTranslation) "#ff9633" else "#00c8ff",
                    fontSize = 16,
                    isDark = settings.isDark,
                ),
                "JS_CONTENT" to javascript(locale),
                "TITLE_CONTENT" to if (settings.isTranslation) "" else """<canvas id="volume-canvas" width="90" height="24"></canvas>""",
                "AUDIO_SELECTOR" to controls(settings.isTranslation, locale),
                "LOADING_ICON" to if (settings.isTranslation) {
                    RealtimeOverlayIcons.TRANSLATION_LOADING
                } else {
                    RealtimeOverlayIcons.TRANSCRIPTION_LOADING
                },
                "PLACEHOLDER_TEXT" to locale.overlay.placeholderText,
                "COPY_TEXT_TITLE" to locale.overlay.copyTextTitle,
                "DECREASE_FONT_TITLE" to locale.overlay.decreaseFontTitle,
                "INCREASE_FONT_TITLE" to locale.overlay.increaseFontTitle,
                "TOGGLE_TRANSCRIPTION_TITLE" to locale.overlay.toggleTranscriptionTitle,
                "TOGGLE_TRANSLATION_TITLE" to locale.overlay.toggleTranslationTitle,
                "TOGGLE_HEADER_TITLE" to locale.overlay.toggleHeaderTitle,
                "CONTENT_COPY_SVG" to RealtimeOverlayIcons.CONTENT_COPY,
                "REMOVE_SVG" to RealtimeOverlayIcons.REMOVE,
                "ADD_SVG" to RealtimeOverlayIcons.ADD,
                "SUBTITLES_SVG" to RealtimeOverlayIcons.SUBTITLES,
                "TRANSLATE_SVG" to RealtimeOverlayIcons.TRANSLATE,
                "EXPAND_LESS_SVG" to RealtimeOverlayIcons.EXPAND_LESS,
                "DOWNLOAD_SVG" to RealtimeOverlayIcons.DOWNLOAD,
                "DOWNLOAD_TITLE" to locale.overlay.downloadingModelTitle,
                "PLEASE_WAIT_TEXT" to locale.overlay.pleaseWaitText,
                "SUPPORTS_ENGLISH" to locale.overlay.parakeetNote,
                "CLOSE_SVG" to RealtimeOverlayIcons.CLOSE,
                "CANCEL_TEXT" to locale.overlay.cancelText,
                "CANCEL_DOWNLOAD_TITLE" to locale.overlay.cancelText,
                "VOLUME_UP_SVG" to RealtimeOverlayIcons.VOLUME_UP,
                "TTS_TITLE" to locale.overlay.ttsTitle,
                "TTS_SPEED" to locale.overlay.ttsSpeed,
                "TTS_AUTO" to locale.overlay.ttsAuto,
                "TTS_VOLUME" to locale.overlay.ttsVolume,
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

    private fun javascript(locale: MobileLocaleText): String {
        return buildString {
            append(overlayBridgePrelude())
            append('\n')
            append(
                baseMainJs
                    .replace("{{FONT_SIZE}}", "16")
                    .replace("{{CHECK_SVG}}", RealtimeOverlayIcons.CHECK)
                    .replace("{{COPY_SVG}}", RealtimeOverlayIcons.CONTENT_COPY),
            )
            append('\n')
            append(baseLogicJs.replace("{{PLACEHOLDER_TEXT}}", locale.overlay.placeholderText))
            append('\n')
            append(overlayMobileShim())
        }
    }

    private fun controls(
        isTranslation: Boolean,
        locale: MobileLocaleText,
    ): String {
        return if (isTranslation) {
            """
            <span class="ctrl-btn speak-btn" id="speak-btn" title="${locale.overlay.ttsSettingsTitle}"><span class="material-symbols-rounded">${RealtimeOverlayIcons.VOLUME_UP}</span></span>
            <div class="btn-group">
                <span class="material-symbols-rounded model-icon" data-value="google-gemma" title="${locale.overlay.gemmaTitle}">${RealtimeOverlayIcons.AUTO_AWESOME}</span>
                <span class="material-symbols-rounded model-icon" data-value="cerebras-oss" title="${locale.overlay.cerebrasTitle}">${RealtimeOverlayIcons.SPEED}</span>
                <span class="material-symbols-rounded model-icon" data-value="google-gtx" title="${locale.overlay.gtxTitle}">${RealtimeOverlayIcons.LANGUAGE}</span>
            </div>
            <button class="language-btn" id="language-select" type="button" title="${locale.overlay.targetLanguageTitle}" data-base-title="${locale.overlay.targetLanguageTitle}" data-language="" data-code="">
                <span id="language-select-code">--</span>
            </button>
            """.trimIndent()
        } else {
            """
            <div class="btn-group">
                <span class="material-symbols-rounded audio-icon" id="mic-btn" data-value="mic" title="${locale.overlay.micInputTitle}">${RealtimeOverlayIcons.MIC}</span>
                <span class="material-symbols-rounded audio-icon" id="device-btn" data-value="device" title="${locale.overlay.deviceAudioTitle}">${RealtimeOverlayIcons.SPEAKER_GROUP}</span>
            </div>
            <div class="btn-group">
                <span class="material-symbols-rounded trans-model-icon" data-value="gemini" title="${locale.overlay.geminiLiveTitle}">${RealtimeOverlayIcons.AUTO_AWESOME}</span>
                <span class="material-symbols-rounded trans-model-icon" data-value="parakeet" title="${locale.overlay.parakeetTitle}">${RealtimeOverlayIcons.BOLT_EN}</span>
            </div>
            """.trimIndent()
        }
    }

    private fun asset(name: String): String {
        return context.assets.open("realtime_overlay/$name").bufferedReader().use { it.readText() }
    }
}
