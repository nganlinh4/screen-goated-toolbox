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
        val overlaySource = loadSourceFile(OVERLAY_JS_SOURCE_PATH).readText()

        assertTrue(repositorySource.contains("if (transcriptionModelId() == RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S)"))
        assertTrue(repositorySource.contains("fun updateTranslationModel(modelId: String)"))
        assertTrue(overlaySource.contains("if (s2sMode) return;"))
        assertTrue(overlaySource.contains("window.ipc.postMessage('translationModel:' + icon.getAttribute('data-value'))"))
    }

    @Test
    fun `s2s rejects tts disable at state and controller boundaries`() {
        val repositorySource = loadSourceFile(REPOSITORY_SOURCE_PATH).readText()
        val controllerSource = loadSourceFile(OVERLAY_CONTROLLER_SOURCE_PATH).readText()

        assertTrue(repositorySource.contains("enabled = settings.enabled || transcriptionModelId() == RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S"))
        assertTrue(controllerSource.contains("if (repository.transcriptionModelId() == RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S)"))
        assertTrue(controllerSource.contains("repository.updateRealtimeTtsSettings(current.copy(enabled = true))"))
    }

    @Test
    fun `s2s timeouts scale with source audio length`() {
        val s2sSource = loadSourceFile(GEMINI_S2S_CLIENT_SOURCE_PATH).readText()

        assertTrue(s2sSource.contains("private fun groupedFirstAudioTimeoutMs("))
        assertTrue(s2sSource.contains("base + sourceAudioMs * 2"))
        assertTrue(s2sSource.contains("coerceIn(5_500L, 30_000L)"))
        assertTrue(s2sSource.contains("private fun groupedHardTimeoutMs("))
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
    fun `s2s overlay tooltips are localized and refreshed without reload`() {
        val overlaySource = loadSourceFile(OVERLAY_JS_SOURCE_PATH).readText()
        val webViewSource = loadSourceFile(OVERLAY_WEBVIEW_SOURCE_PATH).readText()

        assertTrue(webViewSource.contains("put(\"s2sTranslationModelTitle\", overlay.s2sTranslationModelTitle)"))
        assertTrue(webViewSource.contains("put(\"s2sTargetLanguageTitle\", overlay.s2sTargetLanguageTitle)"))
        assertTrue(webViewSource.contains("put(\"directSpeechTitle\", overlay.directSpeechTitle)"))
        assertTrue(webViewSource.contains("put(\"ttsS2sLockedTitle\", overlay.ttsS2sLockedTitle)"))
        assertTrue(overlaySource.contains("overlayLocale.s2sTranslationModelTitle"))
        assertTrue(overlaySource.contains("overlayLocale.s2sTargetLanguageTitle"))
        assertTrue(overlaySource.contains("overlayLocale.directSpeechTitle"))
        assertTrue(overlaySource.contains("overlayLocale.ttsS2sLockedTitle"))
        assertTrue(overlaySource.contains("applyS2sMode(s2sMode);"))
    }

    @Test
    fun `native overlay pickers use localized search hints`() {
        val pickerSource = loadSourceFile(OVERLAY_LANGUAGE_PICKER_SOURCE_PATH).readText()
        val controllerSource = loadSourceFile(OVERLAY_CONTROLLER_SOURCE_PATH).readText()
        val localeSource = loadSourceFile(MOBILE_LOCALE_SOURCE_PATH).readText()

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

    private companion object {
        private const val CLIENT_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiClients.kt"
        private const val RUNTIME_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveSessionRuntime.kt"
        private const val REPOSITORY_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/model/AndroidLiveSessionRepository.kt"
        private const val OVERLAY_CONTROLLER_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayController.kt"
        private const val OVERLAY_WEBVIEW_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/OverlayControllerWebView.kt"
        private const val OVERLAY_LANGUAGE_PICKER_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/OverlayLanguagePicker.kt"
        private const val GEMINI_S2S_CLIENT_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sClient.kt"
        private const val MOBILE_LOCALE_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n/MobileLocaleText.kt"
        private const val OVERLAY_JS_SOURCE_PATH =
            "mobile/androidApp/src/main/assets/realtime_overlay/main.js"
    }
}
