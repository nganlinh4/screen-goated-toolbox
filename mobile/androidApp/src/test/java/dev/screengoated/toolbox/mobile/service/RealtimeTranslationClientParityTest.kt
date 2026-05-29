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
        private const val OVERLAY_JS_SOURCE_PATH =
            "mobile/androidApp/src/main/assets/realtime_overlay/main.js"
    }
}
