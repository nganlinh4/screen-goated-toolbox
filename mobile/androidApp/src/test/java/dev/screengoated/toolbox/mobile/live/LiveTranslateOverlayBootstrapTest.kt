package dev.screengoated.toolbox.mobile.live

import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.model.RealtimePaneFontSizes
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.service.overlay.RealtimeOverlayModelOptions
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LiveTranslateOverlayBootstrapTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `live translate defaults match overlay bootstrap fixture`() {
        val defaults = loadFixture().defaults
        val config = LiveSessionConfig()
        val fontSizes = RealtimePaneFontSizes()
        val tts = RealtimeTtsSettings()

        assertEquals(defaults.audioSource, config.sourceMode.toFixtureValue())
        assertEquals(defaults.targetLanguage, config.targetLanguage)
        assertEquals(defaults.translationModel, config.translationProvider.id)
        assertEquals(defaults.translationApiModel, config.translationProvider.model)
        assertEquals(defaults.transcriptionModel, config.transcriptionProvider.id.toFixtureTranscriptionModel())
        assertEquals(defaults.fontSize, fontSizes.transcriptionSp)
        assertEquals(defaults.fontSize, fontSizes.translationSp)
        assertEquals(defaults.ttsEnabled, tts.enabled)
        assertEquals(defaults.ttsSpeed, tts.speedPercent)
        assertEquals(defaults.ttsAutoSpeed, tts.autoSpeed)
        assertEquals(defaults.ttsVolume, tts.volumePercent)
    }

    @Test
    fun `live translate model catalogs expose fixture-required providers`() {
        val fixture = loadFixture()
        val controls = fixture.requiredControls
        val requiredModels = fixture.requiredModels
        val translation = RealtimeOverlayModelOptions.translationProviderIds
        val transcription = RealtimeOverlayModelOptions.transcriptionProviderIds

        assertEquals(requiredModels.translationProviders, translation)
        assertEquals(requiredModels.androidTranscriptionProviders, transcription)
        assertEquals(requiredModels.targetLanguages, LanguageCatalog.names)
        assertTrue(translation.contains(defaultTranslationProviderId()))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_GEMINI_3_1))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_GEMINI_TRANSLATE))
        assertTrue(!transcription.contains(RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_PARAKEET))
        assertTrue(requiredModels.androidUnavailableTranscriptionProviders.contains(RealtimeModelIds.TRANSCRIPTION_PARAKEET))
        assertTrue(controls.translationPane.contains("translation-model-toggle"))
        assertTrue(controls.transcriptionPane.contains("transcription-model-toggle"))
        assertTrue(fixture.requiredVisuals.androidS2sAdaptiveVad)
        assertTrue(fixture.requiredVisuals.androidS2sScaledTimeouts)
        assertTrue(fixture.requiredVisuals.androidS2sStaleOrderedSkip)
        assertTrue(fixture.requiredVisuals.androidS2sFullTranscriptDisplay)
        assertTrue(fixture.requiredVisuals.androidS2sRejectsTranslationModelChanges)
        assertTrue(fixture.requiredVisuals.androidS2sRejectsTtsDisable)
        assertTrue(fixture.requiredVisuals.androidTextLlmUsesProviderAvailability)
        assertTrue(fixture.requiredVisuals.androidRejectedTranslationApplyIsFailure)
        assertTrue(fixture.requiredVisuals.androidRejectedPrimaryApplyCanTryFallback)
        assertTrue(fixture.requiredVisuals.androidForceCommitPrimesTranslationInterval)
        assertTrue(fixture.requiredVisuals.androidSkipsTranslationWhenPaneHidden)
        assertEquals("live-on-ui-language-change", fixture.requiredVisuals.ttsModalLocaleRefresh)
        assertEquals("live-on-ui-language-change", fixture.requiredVisuals.downloadModalLocaleRefresh)
        assertEquals("live-on-ui-language-change", fixture.requiredVisuals.s2sTooltipLocaleRefresh)
        assertEquals("hidden-not-greyed", fixture.requiredVisuals.irrelevantDisabledControls)
        assertEquals("active-ui-language-bundle", fixture.requiredVisuals.nativePickerLocaleSource)
        assertTrue(fixture.requiredVisuals.targetLanguageChangeRestartsS2s)
    }

    @Test
    fun `active Android model picker labels stay human-readable`() {
        val fixture = loadFixture()
        val transcriptionOptions = RealtimeOverlayModelOptions.transcriptionOptions(
            geminiS2sLabel = "Gemini S2S",
            unavailableSuffix = "Unavailable",
        )
        val translationOptions = RealtimeOverlayModelOptions.translationOptions(
            llmLabel = "LLM",
            gtxLabel = "Google Translate",
        )
        val allOptions = transcriptionOptions + translationOptions

        assertEquals(fixture.requiredModels.androidTranscriptionProviders, transcriptionOptions.map { it.id })
        assertEquals(fixture.requiredModels.translationProviders, translationOptions.map { it.id })
        transcriptionOptions.forEach { option ->
            val shouldBeUnavailable = option.id in fixture.requiredModels.androidUnavailableTranscriptionProviders
            assertEquals("${option.id} enabled state", !shouldBeUnavailable, option.enabled)
        }
        allOptions.forEach { option ->
            assertTrue("Model picker label must not fall back to id: ${option.id}", option.label != option.id)
            assertTrue("Model picker label must not be blank: ${option.id}", option.label.isNotBlank())
        }
        assertEquals("Gemini Live", transcriptionOptions.first { it.id == RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5 }.label)
        assertEquals("Gemini S2S", transcriptionOptions.first { it.id == RealtimeModelIds.TRANSCRIPTION_GEMINI_3_1 }.label)
        assertEquals("Gemini Translate", transcriptionOptions.first { it.id == RealtimeModelIds.TRANSCRIPTION_GEMINI_TRANSLATE }.label)
        assertEquals("Parakeet (Unavailable)", transcriptionOptions.first { it.id == RealtimeModelIds.TRANSCRIPTION_PARAKEET }.label)
        assertEquals("Moonshine Tiny", transcriptionOptions.first { it.id == "moonshine-tiny-streaming" }.label)
        assertEquals("Moonshine Small", transcriptionOptions.first { it.id == "moonshine-small-streaming" }.label)
        assertEquals("Moonshine Medium", transcriptionOptions.first { it.id == "moonshine-medium-streaming" }.label)
        assertEquals("Zipformer", transcriptionOptions.first { it.id == "zipformer" }.label)

        val mainJsSource = loadRepoFile(OVERLAY_MAIN_JS_PATH).readText() +
            loadRepoFile(OVERLAY_MAIN_JS_PART2_PATH).readText()
        assertTrue(mainJsSource.contains("'gemini-live-audio': 'Gemini Live'"))
        assertTrue(mainJsSource.contains("'gemini-live-audio-3.1': 'Gemini S2S'"))
        assertTrue(mainJsSource.contains("'gemini-3.5-translate': 'Gemini Translate'"))
        assertTrue(mainJsSource.contains("'moonshine-tiny-streaming': 'Moonshine Tiny'"))
        assertTrue(mainJsSource.contains("'moonshine-small-streaming': 'Moonshine Small'"))
        assertTrue(mainJsSource.contains("'moonshine-medium-streaming': 'Moonshine Medium'"))
        assertTrue(mainJsSource.contains("'zipformer': 'Zipformer'"))
        assertTrue(mainJsSource.contains("return 'Parakeet (' + (overlayLocale.unavailableSuffix || 'Unavailable') + ')'"))
    }

    @Test
    fun `active Android overlay sources expose fixture-required controls`() {
        val fixture = loadFixture()
        val baseHtml = loadRepoFile(OVERLAY_BASE_HTML_PATH).readText()
        val builderSource = loadRepoFile(OVERLAY_HTML_BUILDER_PATH).readText()
        val logicSource = loadRepoFile(OVERLAY_LOGIC_JS_PATH).readText()
        val styleSource = loadRepoFile(OVERLAY_STYLE_PATH).readText()
        val mainJsSource = loadRepoFile(OVERLAY_MAIN_JS_PATH).readText() +
            loadRepoFile(OVERLAY_MAIN_JS_PART2_PATH).readText()
        val webViewSource = loadRepoFile(OVERLAY_WEBVIEW_PATH).readText()
        val paneWindowSource = loadRepoFile(OVERLAY_PANE_WINDOW_PATH).readText()
        val controllerSource = loadRepoFile(OVERLAY_CONTROLLER_PATH).readText()
        val boundsSource = loadRepoFile(OVERLAY_BOUNDS_PATH).readText()
        val runtimeModelsSource = loadRepoFile(TTS_RUNTIME_MODELS_PATH).readText()
        val audioTrackSource = loadRepoFile(AUDIO_TRACK_PLAYER_PATH).readText()
        val realtimeTtsCoordinatorSource = loadRepoFile(REALTIME_TTS_COORDINATOR_PATH).readText()

        fixture.requiredControls.transcriptionPane.forEach { control ->
            assertTrue("Missing transcription control: $control", activeControlSource(control, baseHtml, builderSource))
        }
        fixture.requiredControls.translationPane.forEach { control ->
            assertTrue("Missing translation control: $control", activeControlSource(control, baseHtml, builderSource))
        }
        assertEquals("Google Sans Flex", fixture.requiredVisuals.fontFamily)
        assertTrue(styleSource.contains("font-family: 'Google Sans Flex'"))
        assertEquals("absent", fixture.requiredVisuals.translationHeaderText)
        assertTrue(builderSource.contains("\"TITLE_CONTENT\" to \"\""))
        assertTrue(styleSource.contains("#title:empty"))
        assertEquals("detached", fixture.requiredVisuals.mobileOverlayWindows)
        assertTrue(controllerSource.contains("internal var transcriptionWindow: OverlayPaneWindow? = null"))
        assertTrue(controllerSource.contains("internal var translationWindow: OverlayPaneWindow? = null"))
        assertEquals("stacked-default-detached", fixture.requiredVisuals.portraitLayout)
        assertTrue(boundsSource.contains("OverlayPaneId.TRANSCRIPTION -> top"))
        assertTrue(boundsSource.contains("OverlayPaneId.TRANSLATION -> (top + height + gap)"))
        assertEquals("side-by-side-default-detached", fixture.requiredVisuals.landscapeLayout)
        assertTrue(boundsSource.contains("OverlayPaneId.TRANSCRIPTION -> margin"))
        assertTrue(boundsSource.contains("OverlayPaneId.TRANSLATION -> (screen.width() - width - margin)"))
        assertEquals(false, fixture.requiredVisuals.chevronConsumesLayoutSpace)
        assertTrue(styleSource.contains("#header-toggle"))
        assertTrue(styleSource.contains("position: absolute"))
        assertEquals("horizontal-swipe", fixture.requiredVisuals.headerOverflowBehavior)
        assertTrue(styleSource.contains("overflow-x: auto"))
        assertEquals("preserve-on-runtime-control-taps", fixture.requiredVisuals.headerScrollRetention)
        assertTrue(mainJsSource.contains("function preserveControlsScroll(callback)"))
        assertTrue(mainJsSource.contains("restoreControlsScroll(pinnedScrollLeft)"))
        assertEquals("discrete-js-no-html-reload", fixture.requiredVisuals.headerStateSync)
        assertEquals("discrete-js-no-html-reload", fixture.requiredVisuals.localeUpdatePath)
        assertTrue(webViewSource.contains("if(window.setLocaleStrings) window.setLocaleStrings("))
        assertTrue(webViewSource.contains("if(window.setTargetLanguage) window.setTargetLanguage("))
        assertTrue(webViewSource.contains("if(window.setTranslationModel) window.setTranslationModel("))
        assertTrue(webViewSource.contains("if(window.setTranscriptionModel) window.setTranscriptionModel("))
        assertTrue(mainJsSource.contains("translationBtn.hidden = s2sMode"))
        assertTrue(mainJsSource.contains("transLangBadge.hidden = true"))
        assertTrue(mainJsSource.contains("transLangBadge.hidden = false"))
        assertEquals("windows-fulltext-previousFullText-commitAdvance", fixture.requiredVisuals.textRendererContract)
        assertTrue(builderSource.contains(".replace(\"{{ENABLE_INLINE_DIFF}}\", isTranslation.toString())"))
        assertTrue(logicSource.contains("const fullText = oldText + newText;"))
        assertTrue(logicSource.contains("const previousCommittedLength = currentOldTextLength;"))
        assertTrue(logicSource.contains("const committedAdvanced = oldText.length > previousCommittedLength;"))
        assertTrue(logicSource.contains("function renderCommitAdvance(fullText, previousCommittedLength, committedLength)"))
        assertTrue(logicSource.contains("const prefixText = fullText.substring(0, previousCommittedLength);"))
        assertTrue(logicSource.contains("const promotingText = fullText.substring(previousCommittedLength, committedLength);"))
        assertTrue(logicSource.contains("const suffixText = fullText.substring(committedLength);"))
        assertTrue(logicSource.contains("previousFullText"))
        assertTrue(logicSource.contains("inlineDiffEnabled && canAnimateWordDiff(previousFullText, fullText)"))
        assertEquals("active-ui-language-bundle", fixture.requiredVisuals.overlayLocaleSource)
        assertEquals("live-on-ui-language-change", fixture.requiredVisuals.overlayLocaleRefresh)
        assertTrue(webViewSource.contains("localeJson = overlayLocaleJson(uiLanguage)"))
        assertTrue(mainJsSource.contains("window.setLocaleStrings = function(locale)"))
        assertTrue(fixture.requiredVisuals.roundedWindowMask)
        assertTrue(paneWindowSource.contains("clipToOutline = true"))
        assertTrue(paneWindowSource.contains("outline.setRoundRect"))
        assertEquals("native-overlay", fixture.requiredVisuals.languagePicker)
        assertEquals("active-ui-language-bundle", fixture.requiredVisuals.nativePickerLocaleSource)
        assertTrue(controllerSource.contains("OverlayLanguagePicker("))
        assertEquals("non-capturable-no-app-picker", fixture.requiredVisuals.androidDeviceTtsIsolation)
        assertTrue(audioTrackSource.contains("AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK"))
        assertTrue(audioTrackSource.contains("setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_NONE)"))
        assertTrue(fixture.requiredVisuals.androidTtsSettingsApplyToUnreadText)
        assertTrue(realtimeTtsCoordinatorSource.contains("if (normalized.length < spokenLength || normalized.length < queuedLength)"))
        assertTrue(realtimeTtsCoordinatorSource.contains("settingsSnapshot = globalSettings.toRuntimeSnapshot("))
        assertTrue(fixture.requiredVisuals.androidTtsRequestsTransientDuckFocus)
        assertEquals("restore-pre-drag", fixture.requiredVisuals.dismissDragPersistence)
        assertTrue(controllerSource.contains("boundsPersistenceSuspended.add(paneId)"))
        assertTrue(controllerSource.contains("boundsPersistenceSuspended.remove(paneId)"))
        assertEquals("pinch", fixture.requiredVisuals.mobileResizeGesture)
        assertTrue(!baseHtml.contains("id=\"resize-hint\""))
        assertTrue(!styleSource.contains("#resize-hint"))
        assertEquals("runtime-effective-while-speaking", fixture.requiredVisuals.ttsDisplayedSpeedSource)
        assertTrue(controllerSource.contains("runtimeState.currentRealtimeEffectiveSpeed.coerceIn(50, 200)"))
        assertTrue(controllerSource.contains("runtimeState.activeConsumer == TtsConsumer.REALTIME"))
        assertTrue(runtimeModelsSource.contains("currentRealtimeEffectiveSpeed: Int = 100"))
    }

    @Test
    fun `active Android controller restarts only S2S target language changes`() {
        val fixture = loadFixture()
        val controllerSource = loadRepoFile(OVERLAY_CONTROLLER_PATH).readText()
        val paritySource = loadRepoFile(LIVE_TRANSLATE_PARITY_PATH).readText()

        assertTrue(fixture.requiredVisuals.targetLanguageChangeRestartsS2s)
        assertTrue(controllerSource.contains("internal fun updateTargetLanguage(language: String)"))
        assertTrue(controllerSource.contains("LiveTranslateParity.targetLanguageChangeRequiresRestart("))
        assertTrue(controllerSource.contains("previousConfig.transcriptionProvider.id"))
        assertTrue(controllerSource.contains("restartRequested()"))
        assertTrue(paritySource.contains("transcriptionProviderId == \"gemini-live-s2s\""))
        assertTrue(paritySource.contains("transcriptionProviderId == \"gemini-3.5-translate\""))
    }

    @Test
    fun `overlay bootstrap fixture visual contract is fully modeled by Android tests`() {
        val rawFixture = json.decodeFromString<JsonObject>(loadRepoFile(FIXTURE_PATH).readText())
        val visualKeys = rawFixture.getValue("requiredVisuals").jsonObject.keys

        assertEquals(
            setOf(
                "fontFamily",
                "translationHeaderText",
                "mobileResizeGesture",
                "mobileOverlayWindows",
                "portraitLayout",
                "landscapeLayout",
                "chevronConsumesLayoutSpace",
                "headerOverflowBehavior",
                "headerScrollRetention",
                "headerStateSync",
                "textRendererContract",
                "overlayLocaleSource",
                "overlayLocaleRefresh",
                "ttsModalLocaleRefresh",
                "downloadModalLocaleRefresh",
                "s2sTooltipLocaleRefresh",
                "irrelevantDisabledControls",
                "roundedWindowMask",
                "languagePicker",
                "nativePickerLocaleSource",
                "androidDeviceTtsIsolation",
                "androidTtsSettingsApplyToUnreadText",
                "androidTtsRequestsTransientDuckFocus",
                "androidS2sAdaptiveVad",
                "androidS2sScaledTimeouts",
                "androidS2sStaleOrderedSkip",
                "androidS2sFullTranscriptDisplay",
                "androidS2sRejectsTranslationModelChanges",
                "androidS2sRejectsTtsDisable",
                "androidTextLlmUsesProviderAvailability",
                "androidRejectedTranslationApplyIsFailure",
                "androidRejectedPrimaryApplyCanTryFallback",
                "androidForceCommitPrimesTranslationInterval",
                "androidSkipsTranslationWhenPaneHidden",
                "targetLanguageChangeRestartsS2s",
                "ttsDisplayedSpeedSource",
                "localeUpdatePath",
                "dismissDragPersistence",
            ),
            visualKeys,
        )
    }

    private fun defaultTranslationProviderId(): String = LiveSessionConfig().translationProvider.id

    private fun SourceMode.toFixtureValue(): String {
        return when (this) {
            SourceMode.DEVICE -> "device"
            SourceMode.MIC -> "mic"
        }
    }

    private fun String.toFixtureTranscriptionModel(): String {
        return when (this) {
            RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5,
            RealtimeModelIds.TRANSCRIPTION_GEMINI_3_1,
            -> "gemini"
            RealtimeModelIds.TRANSCRIPTION_PARAKEET -> "parakeet"
            else -> this
        }
    }

    private fun loadFixture(): OverlayFixture {
        return json.decodeFromString(loadRepoFile(FIXTURE_PATH).readText())
    }

    private fun loadRepoFile(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, path) }
            .firstOrNull(File::exists)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private fun activeControlSource(
        control: String,
        baseHtml: String,
        builderSource: String,
    ): Boolean {
        val source = "$baseHtml\n$builderSource"
        return when (control) {
            "waveform-canvas" -> source.contains("id=\"volume-canvas\"")
            "audio-source-toggle" -> source.contains("id=\"mic-btn\"") && source.contains("id=\"device-btn\"")
            "transcription-model-toggle" -> source.contains("id=\"transcription-model-btn\"")
            "tts-read" -> source.contains("id=\"speak-btn\"")
            "translation-model-toggle" -> source.contains("id=\"translation-model-btn\"")
            "language-select" -> source.contains("id=\"language-select\"")
            "copy" -> source.contains("id=\"copy-btn\"")
            "font-minus" -> source.contains("id=\"font-decrease\"")
            "font-plus" -> source.contains("id=\"font-increase\"")
            "toggle-transcription" -> source.contains("id=\"toggle-mic\"")
            "toggle-translation" -> source.contains("id=\"toggle-trans\"")
            "collapse-chevron" -> source.contains("id=\"header-toggle\"")
            else -> false
        }
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/live-translate/overlay-bootstrap.json"
        private const val OVERLAY_BASE_HTML_PATH = "mobile/androidApp/src/main/assets/realtime_overlay/base.html"
        private const val OVERLAY_MAIN_JS_PATH = "mobile/androidApp/src/main/assets/realtime_overlay/main.js"
        private const val OVERLAY_MAIN_JS_PART2_PATH =
            "mobile/androidApp/src/main/assets/realtime_overlay/main_part2.js"
        private const val OVERLAY_LOGIC_JS_PATH = "mobile/androidApp/src/main/assets/realtime_overlay/logic.js"
        private const val OVERLAY_STYLE_PATH = "mobile/androidApp/src/main/assets/realtime_overlay/style.css"
        private const val OVERLAY_HTML_BUILDER_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/RealtimeOverlayHtmlBuilder.kt"
        private const val OVERLAY_WEBVIEW_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayControllerWebView.kt"
        private const val OVERLAY_PANE_WINDOW_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/OverlayPaneWindow.kt"
        private const val OVERLAY_CONTROLLER_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayController.kt"
        private const val OVERLAY_BOUNDS_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayControllerBounds.kt"
        private const val TTS_RUNTIME_MODELS_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts/TtsRuntimeModels.kt"
        private const val AUDIO_TRACK_PLAYER_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts/AudioTrackPlayer.kt"
        private const val REALTIME_TTS_COORDINATOR_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts/RealtimeTtsCoordinator.kt"
        private const val LIVE_TRANSLATE_PARITY_PATH =
            "mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/live/LiveTranslateParity.kt"
    }
}

@Serializable
private data class OverlayFixture(
    val defaults: OverlayDefaults,
    val requiredModels: RequiredModels,
    val requiredControls: RequiredControls,
    val requiredVisuals: RequiredVisuals,
)

@Serializable
private data class OverlayDefaults(
    val audioSource: String,
    val targetLanguage: String,
    val translationModel: String,
    val translationApiModel: String,
    val transcriptionModel: String,
    val fontSize: Int,
    val ttsEnabled: Boolean,
    val ttsSpeed: Int,
    val ttsAutoSpeed: Boolean,
    val ttsVolume: Int,
)

@Serializable
private data class RequiredModels(
    val translationProviders: List<String>,
    val windowsTranscriptionProviders: List<String>,
    val androidTranscriptionProviders: List<String>,
    val androidUnavailableTranscriptionProviders: List<String>,
    val targetLanguages: List<String>,
)

@Serializable
private data class RequiredControls(
    val transcriptionPane: List<String>,
    val translationPane: List<String>,
)

@Serializable
private data class RequiredVisuals(
    val fontFamily: String,
    val translationHeaderText: String,
    val mobileResizeGesture: String,
    val mobileOverlayWindows: String,
    val portraitLayout: String,
    val landscapeLayout: String,
    val chevronConsumesLayoutSpace: Boolean,
    val headerOverflowBehavior: String,
    val headerScrollRetention: String,
    val headerStateSync: String,
    val textRendererContract: String,
    val overlayLocaleSource: String,
    val overlayLocaleRefresh: String,
    val androidS2sAdaptiveVad: Boolean,
    val androidS2sScaledTimeouts: Boolean,
    val androidS2sStaleOrderedSkip: Boolean,
    val androidS2sFullTranscriptDisplay: Boolean,
    val androidS2sRejectsTranslationModelChanges: Boolean,
    val androidS2sRejectsTtsDisable: Boolean,
    val androidTextLlmUsesProviderAvailability: Boolean,
    val androidRejectedTranslationApplyIsFailure: Boolean,
    val androidRejectedPrimaryApplyCanTryFallback: Boolean,
    val androidForceCommitPrimesTranslationInterval: Boolean,
    val androidSkipsTranslationWhenPaneHidden: Boolean,
    val ttsModalLocaleRefresh: String,
    val downloadModalLocaleRefresh: String,
    val s2sTooltipLocaleRefresh: String,
    val irrelevantDisabledControls: String,
    val roundedWindowMask: Boolean,
    val languagePicker: String,
    val nativePickerLocaleSource: String,
    val androidDeviceTtsIsolation: String,
    val androidTtsSettingsApplyToUnreadText: Boolean,
    val androidTtsRequestsTransientDuckFocus: Boolean,
    val targetLanguageChangeRestartsS2s: Boolean,
    val ttsDisplayedSpeedSource: String,
    val localeUpdatePath: String,
    val dismissDragPersistence: String,
)
