package dev.screengoated.toolbox.mobile.service.overlay

import android.content.Context
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.icons.MaterialSymbolSvgs

internal data class RealtimeOverlayPaneSettings(
    val isTranslation: Boolean,
    val isDark: Boolean,
)

internal class RealtimeOverlayHtmlBuilder(
    private val context: Context,
) {
    private val baseHtml by lazy { asset("base.html") }
    private val baseCss by lazy { asset("style.css") }
    private val baseMainJs by lazy { asset("main.js") + asset("main_part2.js") }
    private val baseLogicJs by lazy { asset("logic.js") }
    private val symbols = MaterialSymbolSvgs.fromAssets(context)
    private val htmlCache = LinkedHashMap<RealtimeOverlayPaneSettings, String>()

    fun build(settings: RealtimeOverlayPaneSettings): String {
        return htmlCache.getOrPut(settings) {
            val locale = MobileLocaleText.forLanguage(DEFAULT_TEMPLATE_LANGUAGE)
            val replacements = linkedMapOf(
                "FONT_CSS" to overlayFontCss(),
                "CSS_CONTENT" to overlayCss(
                    baseCss = baseCss,
                    glowColor = if (settings.isTranslation) "#ff9633" else "#00c8ff",
                    fontSize = 16,
                    isDark = settings.isDark,
                ),
                "JS_CONTENT" to javascript(locale, settings.isTranslation),
                "TITLE_CONTENT" to "",
                "AUDIO_SELECTOR" to controls(settings.isTranslation, locale),
                "LOADING_ICON" to loadingIcon(settings.isTranslation),
                "PLACEHOLDER_TEXT" to locale.overlay.placeholderText,
                "COPY_TEXT_TITLE" to locale.overlay.copyTextTitle,
                "DECREASE_FONT_TITLE" to locale.overlay.decreaseFontTitle,
                "INCREASE_FONT_TITLE" to locale.overlay.increaseFontTitle,
                "TOGGLE_TRANSCRIPTION_TITLE" to locale.overlay.toggleTranscriptionTitle,
                "TOGGLE_TRANSLATION_TITLE" to locale.overlay.toggleTranslationTitle,
                "TOGGLE_HEADER_TITLE" to locale.overlay.toggleHeaderTitle,
                "CONTENT_COPY_SVG" to materialSymbol("content_copy"),
                "REMOVE_SVG" to materialSymbol("remove"),
                "ADD_SVG" to materialSymbol("add"),
                "SUBTITLES_SVG" to materialSymbol("subtitles"),
                "TRANSLATE_SVG" to materialSymbol("translate"),
                "EXPAND_LESS_SVG" to materialSymbol("expand_less"),
                "CLOSE_SVG" to materialSymbol("close"),
                "VOLUME_UP_SVG" to materialSymbol("volume_up"),
                "TTS_TITLE" to locale.overlay.ttsTitle,
                "TTS_SPEED" to locale.overlay.ttsSpeed,
                "TTS_AUTO" to locale.overlay.ttsAuto,
                "TTS_VOLUME" to locale.overlay.ttsVolume,
                "DICTIONARY_SVG" to materialSymbol("dictionary"),
                "CUSTOM_VOCABULARY_TITLE" to locale.overlay.customVocabularyTitle,
                "CUSTOM_VOCABULARY_PROMPT" to locale.overlay.customVocabularyPrompt,
            )
            replacements.entries.fold(baseHtml) { html, (token, value) ->
                html.replace("{{$token}}", value)
            }
        }
    }

    private fun javascript(
        locale: MobileLocaleText,
        isTranslation: Boolean,
    ): String {
        return buildString {
            append(overlayBridgePrelude())
            append('\n')
            append(
                baseMainJs
                    .replace("{{FONT_SIZE}}", "16")
                    .replace("{{CHECK_SVG}}", materialSymbol("check"))
                    .replace("{{COPY_SVG}}", materialSymbol("content_copy")),
            )
            append('\n')
            append(
                baseLogicJs
                    .replace("{{PLACEHOLDER_TEXT}}", locale.overlay.placeholderText)
                    .replace("{{ENABLE_INLINE_DIFF}}", isTranslation.toString()),
            )
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
            <span class="ctrl-btn speak-btn" id="speak-btn" title="${locale.overlay.ttsSettingsTitle}"><span class="inline-svg-icon">${materialSymbol("volume_up")}</span></span>
            <button class="model-dropdown" id="translation-model-btn" type="button" title="${locale.overlay.translationModelTitle}" data-value="">
                <span id="translation-model-label">--</span>
            </button>
            <button class="language-btn" id="language-select" type="button" title="${locale.overlay.targetLanguageTitle}" data-base-title="${locale.overlay.targetLanguageTitle}" data-language="" data-code="">
                <span id="language-select-code">--</span>
            </button>
            """.trimIndent()
        } else {
            """
            <canvas id="volume-canvas" width="90" height="24"></canvas>
            <div class="btn-group">
                <span class="inline-svg-icon audio-icon" id="mic-btn" data-value="mic" title="${locale.overlay.micInputTitle}">${materialSymbol("mic")}</span>
                <span class="inline-svg-icon audio-icon" id="device-btn" data-value="device" title="${locale.overlay.deviceAudioTitle}">${materialSymbol("speaker_group")}</span>
            </div>
            <button class="model-dropdown" id="transcription-model-btn" type="button" title="${locale.overlay.transcriptionModelTitle}" data-value="">
                <span id="transcription-model-label">--</span>
            </button>
            <button class="ctrl-btn" id="custom-vocabulary-btn" type="button" title="${locale.overlay.customVocabularyTitle}" hidden>
                <span class="inline-svg-icon">${materialSymbol("dictionary")}</span>
            </button>
            """.trimIndent()
        }
    }

    private fun asset(name: String): String {
        return context.assets.open("realtime_overlay/$name").bufferedReader().use { it.readText() }
    }

    private fun materialSymbol(name: String): String = symbols.svg(name)

    /** The pane's symbol, tinted with its accent color and animated by `.loading-svg`. */
    private fun loadingIcon(isTranslation: Boolean): String {
        val (svg, color) = if (isTranslation) {
            materialSymbol("translate") to "#ff9633"
        } else {
            materialSymbol("graphic_eq") to "#00c8ff"
        }
        return svg.replaceFirst("<svg ", """<svg class="loading-svg" style="color: $color" """)
    }

    private companion object {
        const val DEFAULT_TEMPLATE_LANGUAGE = "en"
    }
}
