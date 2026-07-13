package dev.screengoated.toolbox.mobile.service

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class RealtimeTranslationClientParityTest {
    @Test
    fun `text llm chain uses shared provider availability gates`() {
        val clientSource = loadSourceFile(CLIENT_SOURCE_PATH).readText()
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()

        assertTrue(clientSource.contains("import dev.screengoated.toolbox.mobile.preset.providerIsAvailable"))
        assertTrue(clientSource.contains("providerIsAvailable(descriptor.provider, apiKeys, runtimeSettings)"))
        assertTrue(runtimeSource.contains("runtimeSettings = repository.currentPresetRuntimeSettings()"))
    }

    @Test
    fun `s2s rejects translation model changes at state and legacy ipc boundaries`() {
        val repositorySource = loadSourceFile(REPOSITORY_SOURCE_PATH).readText()
        val overlaySource = loadOverlayJsSource()

        assertTrue(repositorySource.contains("RealtimeModelIds.isGeminiS2sModelId(transcriptionModelId())"))
        assertTrue(repositorySource.contains("fun updateTranslationModel(modelId: String)"))
        assertTrue(overlaySource.contains("if (s2sMode) return;"))
        assertTrue(overlaySource.contains("window.ipc.postMessage('translationModel:' + icon.getAttribute('data-value'))"))
    }

    @Test
    fun `s2s rejects tts disable at state and controller boundaries`() {
        val repositorySource = loadSourceFile(REPOSITORY_SOURCE_PATH).readText()
        val controllerSource = loadSourceFile(OVERLAY_CONTROLLER_SOURCE_PATH).readText()

        assertTrue(repositorySource.contains("enabled = settings.enabled || RealtimeModelIds.isGeminiS2sModelId(transcriptionModelId())"))
        assertTrue(controllerSource.contains("if (RealtimeModelIds.isGeminiS2sModelId(repository.transcriptionModelId()))"))
        assertTrue(controllerSource.contains("repository.updateRealtimeTtsSettings(current.copy(enabled = true))"))
    }

    @Test
    fun `s2s timeouts scale with source audio length`() {
        val s2sSource = loadSourceFile(GEMINI_S2S_VAD_SOURCE_PATH).readText()

        assertTrue(s2sSource.contains("fun groupedFirstAudioTimeoutMs("))
        assertTrue(s2sSource.contains("base + sourceAudioMs * 2"))
        assertTrue(s2sSource.contains("coerceIn(5_500L, 30_000L)"))
        assertTrue(s2sSource.contains("fun groupedHardTimeoutMs("))
        assertTrue(s2sSource.contains("S2S_HEDGE_TIMEOUT_MS"))
        assertTrue(s2sSource.contains("S2S_HEDGE_FINAL_TIMEOUT_MS"))
        assertTrue(s2sSource.contains("base + sourceAudioMs * 4"))
        assertTrue(s2sSource.contains("coerceAtMost(180_000L)"))
    }

    @Test
    fun `force commit primes the next translation interval`() {
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()

        assertTrue(runtimeSource.contains("if (repository.forceCommitIfDue(nowMs))"))
        assertTrue(runtimeSource.contains("lastTranslationAttemptAtMs = (nowMs - translationIntervalMs).coerceAtLeast(0L)"))
    }

    @Test
    fun `translation cadence stays adaptive and failure keeps session alive`() {
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()

        assertTrue(runtimeSource.contains("const val TRANSLATION_INTERVAL_MS = 1_500L"))
        assertTrue(runtimeSource.contains("const val TRANSLATION_INTERVAL_MAX_MS = 4_000L"))
        assertTrue(runtimeSource.contains("fun computeAdaptiveTranslationIntervalMs(latencyMs: Long): Long"))
        assertTrue(runtimeSource.contains("return (latencyMs + 250L)"))
        assertTrue(runtimeSource.contains(".coerceAtLeast(TRANSLATION_INTERVAL_MS)"))
        assertTrue(runtimeSource.contains(".coerceAtMost(TRANSLATION_INTERVAL_MAX_MS)"))
        assertTrue(runtimeSource.contains("translationIntervalMs = computeAdaptiveTranslationIntervalMs(latencyMs)"))
        assertTrue(runtimeSource.contains("catch (error: Throwable)"))
        assertTrue(runtimeSource.contains("Translation failure should not kill the session"))
        assertTrue(runtimeSource.contains("translationIntervalMs = (translationIntervalMs + 250L).coerceAtMost(TRANSLATION_INTERVAL_MAX_MS)"))
        assertTrue(runtimeSource.contains("repository.markListening()"))
        assertTrue(!runtimeSource.contains("repository.fail(error.message ?: \"Translation"))
    }

    @Test
    fun `hidden translation pane skips provider requests and realtime tts`() {
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()
        val controllerSource = loadSourceFile(OVERLAY_CONTROLLER_SOURCE_PATH).readText()

        assertTrue(controllerSource.contains("fun isTranslationVisible(): Boolean = translationVisible"))
        assertTrue(runtimeSource.contains("if (!overlayController.isTranslationVisible())"))
        assertTrue(runtimeSource.contains("lastTranslationAttemptAtMs = nowMs"))
        assertTrue(runtimeSource.contains("val request = repository.claimTranslationRequest()"))
        assertTrue(runtimeSource.indexOf("if (!overlayController.isTranslationVisible())") < runtimeSource.indexOf("val request = repository.claimTranslationRequest()"))
        assertTrue(runtimeSource.contains("realtimeTtsCoordinator.stop()"))
    }

    @Test
    fun `translation success requires accepted state apply`() {
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()
        val repositorySource = loadSourceFile(REPOSITORY_SOURCE_PATH).readText()
        val storeSource = loadSourceFile(LIVE_SESSION_STORE_SOURCE_PATH).readText()

        assertTrue(storeSource.contains("): Boolean"))
        assertTrue(storeSource.contains("applied = liveText != current.liveText"))
        assertTrue(repositorySource.contains("): Boolean"))
        assertTrue(runtimeSource.contains("applied = repository.applyTranslationResponse("))
        assertTrue(runtimeSource.contains("if (!applied)"))
        assertTrue(runtimeSource.contains("Translation response was rejected by the current transcript state."))
    }

    @Test
    fun `translation fallback can run after rejected primary apply`() {
        val clientSource = loadSourceFile(CLIENT_SOURCE_PATH).readText()
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()

        assertTrue(clientSource.contains("suspend fun translateWithExactProvider("))
        assertTrue(runtimeSource.contains("if (!applied && usedProvider == requestedProvider)"))
        assertTrue(runtimeSource.contains("fallbackTranslationProviderId(requestedProvider)"))
        assertTrue(runtimeSource.contains("translationClient.translateWithExactProvider("))
        assertTrue(runtimeSource.contains("usedProvider = result.providerId"))
        assertTrue(runtimeSource.contains("applied = repository.applyTranslationResponse("))
    }

    @Test
    fun `fallback switch is persisted only after accepted fallback response`() {
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()

        val rejectedPrimaryIndex = runtimeSource.indexOf("if (!applied && usedProvider == requestedProvider)")
        val fallbackApplyIndex = runtimeSource.indexOf("applied = repository.applyTranslationResponse(", rejectedPrimaryIndex + 1)
        val rejectedFallbackIndex = runtimeSource.indexOf("if (!applied)", fallbackApplyIndex)
        val persistSwitchIndex = runtimeSource.indexOf("repository.updateTranslationModel(usedProvider)")

        assertTrue(rejectedPrimaryIndex >= 0)
        assertTrue(fallbackApplyIndex > rejectedPrimaryIndex)
        assertTrue(rejectedFallbackIndex > fallbackApplyIndex)
        assertTrue(persistSwitchIndex > rejectedFallbackIndex)
        assertTrue(runtimeSource.contains("repository.translationModelId() == requestedProvider"))
    }

    @Test
    fun `s2s overlay tooltips are localized and refreshed without reload`() {
        val overlaySource = loadOverlayJsSource()
        val webViewSource = loadSourceFile(OVERLAY_WEBVIEW_SOURCE_PATH).readText()
        val builderSource = loadSourceFile(OVERLAY_HTML_BUILDER_SOURCE_PATH).readText()
        val htmlTemplateSource = loadSourceFile(OVERLAY_BASE_HTML_SOURCE_PATH).readText()
        val overlayStyleSource = loadSourceFile(OVERLAY_STYLE_SOURCE_PATH).readText()

        assertTrue(webViewSource.contains("put(\"s2sTranslationModelTitle\", overlay.s2sTranslationModelTitle)"))
        assertTrue(webViewSource.contains("put(\"s2sTargetLanguageTitle\", overlay.s2sTargetLanguageTitle)"))
        assertTrue(webViewSource.contains("put(\"directSpeechTitle\", overlay.directSpeechTitle)"))
        assertTrue(webViewSource.contains("put(\"ttsS2sLockedTitle\", overlay.ttsS2sLockedTitle)"))
        assertTrue(webViewSource.contains("put(\"unavailableSuffix\", overlay.unavailableSuffix)"))
        assertTrue(builderSource.contains("private val baseHtml by lazy { asset(\"base.html\") }"))
        assertTrue(builderSource.contains("val locale = MobileLocaleText.forLanguage(DEFAULT_TEMPLATE_LANGUAGE)"))
        assertTrue(!builderSource.contains("val uiLanguage: String"))
        assertTrue(htmlTemplateSource.contains("title=\"{{COPY_TEXT_TITLE}}\""))
        assertTrue(htmlTemplateSource.contains("title=\"{{TOGGLE_HEADER_TITLE}}\""))
        assertTrue(htmlTemplateSource.contains("id=\"auto-speed-toggle\" title=\"{{TTS_AUTO}}\""))
        assertTrue(!htmlTemplateSource.contains("Auto-adjust speed to catch up"))
        assertTrue(!htmlTemplateSource.contains("id=\"resize-hint\""))
        assertTrue(!overlayStyleSource.contains("#resize-hint"))
        assertTrue(htmlTemplateSource.contains("id=\"tts-modal-title-text\""))
        assertTrue(htmlTemplateSource.contains("id=\"tts-speed-label\""))
        assertTrue(htmlTemplateSource.contains("id=\"tts-volume-label\""))
        assertTrue(htmlTemplateSource.contains("id=\"download-cancel-text\""))
        assertTrue(overlaySource.contains("overlayLocale.s2sTranslationModelTitle"))
        assertTrue(overlaySource.contains("overlayLocale.s2sTargetLanguageTitle"))
        assertTrue(overlaySource.contains("overlayLocale.directSpeechTitle"))
        assertTrue(overlaySource.contains("overlayLocale.ttsS2sLockedTitle"))
        assertTrue(overlaySource.contains("overlayLocale.unavailableSuffix"))
        assertTrue(overlaySource.contains("applyS2sMode(s2sMode);"))
    }

    @Test
    fun `android parakeet remains visibly unavailable and cannot run as fake active transcription`() {
        val runtimeSource = loadSourceFile(RUNTIME_SOURCE_PATH).readText()
        val overlaySource = loadOverlayJsSource()
        val modelOptionsSource = loadSourceFile(OVERLAY_MODEL_OPTIONS_SOURCE_PATH).readText()

        assertTrue(modelOptionsSource.contains("RealtimeModelIds.TRANSCRIPTION_PARAKEET"))
        assertTrue(modelOptionsSource.contains("parakeetLabel(unavailableSuffix)"))
        assertTrue(overlaySource.contains("modelName === 'parakeet'"))
        assertTrue(overlaySource.contains("'Parakeet (' + (overlayLocale.unavailableSuffix || 'Unavailable') + ')'"))
        assertTrue(runtimeSource.contains("config.transcriptionProvider.id == RealtimeModelIds.TRANSCRIPTION_PARAKEET"))
        assertTrue(runtimeSource.contains("Parakeet is visible for Windows parity but is not available on Android yet."))
    }

    @Test
    fun `native overlay pickers use localized search hints`() {
        val pickerSource = loadSourceFile(OVERLAY_LANGUAGE_PICKER_SOURCE_PATH).readText()
        val controllerSource = loadSourceFile(OVERLAY_CONTROLLER_PICKERS_SOURCE_PATH).readText()
        val localeSource = loadSourceFile(MOBILE_OVERLAY_LOCALE_SOURCE_PATH).readText()

        assertTrue(pickerSource.contains("searchHint: String"))
        assertTrue(pickerSource.contains("hint = searchHint"))
        assertTrue(controllerSource.contains("searchHint = locale.overlay.pickerSearchHint"))
        assertTrue(controllerSource.contains("searchHint = overlayLocale.pickerSearchHint"))
        assertTrue(localeSource.contains("val pickerSearchHint: String"))
    }

    private fun loadSourceFile(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, path) }
            .firstOrNull(File::exists)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private fun loadOverlayJsSource(): String =
        loadSourceFile(OVERLAY_JS_SOURCE_PATH).readText() +
            loadSourceFile(OVERLAY_JS_PART2_SOURCE_PATH).readText()

    private companion object {
        private const val CLIENT_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/RealtimeTranslationClient.kt"
        private const val RUNTIME_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveSessionRuntime.kt"
        private const val REPOSITORY_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/model/AndroidLiveSessionRepository.kt"
        private const val LIVE_SESSION_STORE_SOURCE_PATH =
            "mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/live/LiveSessionStore.kt"
        private const val OVERLAY_CONTROLLER_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayController.kt"
        private const val OVERLAY_CONTROLLER_PICKERS_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayControllerPickers.kt"
        private const val OVERLAY_WEBVIEW_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayControllerWebView.kt"
        private const val OVERLAY_LANGUAGE_PICKER_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/OverlayLanguagePicker.kt"
        private const val OVERLAY_HTML_BUILDER_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/RealtimeOverlayHtmlBuilder.kt"
        private const val OVERLAY_BASE_HTML_SOURCE_PATH =
            "mobile/androidApp/src/main/assets/realtime_overlay/base.html"
        private const val OVERLAY_STYLE_SOURCE_PATH =
            "mobile/androidApp/src/main/assets/realtime_overlay/style.css"
        private const val OVERLAY_MODEL_OPTIONS_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/RealtimeOverlayModelOptions.kt"
        private const val GEMINI_S2S_CLIENT_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sClient.kt"
        private const val GEMINI_S2S_VAD_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sVad.kt"
        private const val MOBILE_LOCALE_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n/MobileLocaleText.kt"
        private const val MOBILE_OVERLAY_LOCALE_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n/MobileOverlayLocale.kt"
        private const val OVERLAY_JS_SOURCE_PATH =
            "mobile/androidApp/src/main/assets/realtime_overlay/main.js"
        private const val OVERLAY_JS_PART2_SOURCE_PATH =
            "mobile/androidApp/src/main/assets/realtime_overlay/main_part2.js"
    }
}
