package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.inputAdapter
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PresetGraphExecutorTest {
    private val dispatcher = StandardTestDispatcher()

    @Test
    fun inputAdapterImageOverlayUsesHostedImageHtml() = runTest(dispatcher) {
        val state = MutableStateFlow(PresetExecutionState())
        val executor = createExecutor(state)

        executor.executeGraph(
            sessionId = "test-image",
            preset = overlayOnlyPreset("preset_hang_image"),
            input = PresetInput.Image(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)),
        )

        val window = state.value.resultWindows.single()
        assertTrue(window.markdownText.contains("<img"))
        assertTrue(window.markdownText.contains("data:image/png;base64"))
        assertTrue(window.markdownText.contains("""data-sgt-input-adapter-media="image""""))
    }

    @Test
    fun inputAdapterAudioOverlayUsesHostedAudioHtml() = runTest(dispatcher) {
        val state = MutableStateFlow(PresetExecutionState())
        val executor = createExecutor(state)

        executor.executeGraph(
            sessionId = "test-audio",
            preset = overlayOnlyPreset("preset_quick_record"),
            input = PresetInput.Audio(
                byteArrayOf(
                    'R'.code.toByte(),
                    'I'.code.toByte(),
                    'F'.code.toByte(),
                    'F'.code.toByte(),
                    0, 0, 0, 0,
                    'W'.code.toByte(),
                    'A'.code.toByte(),
                    'V'.code.toByte(),
                    'E'.code.toByte(),
                ),
            ),
        )

        val window = state.value.resultWindows.single()
        assertTrue(window.markdownText.contains("""class="audio-player""""))
        assertTrue(window.markdownText.contains("<audio"))
        assertTrue(window.markdownText.contains("data:audio/wav;base64"))
        assertTrue(window.markdownText.contains("""data-sgt-input-adapter-media="audio""""))
    }

    private fun createExecutor(state: MutableStateFlow<PresetExecutionState>): PresetGraphExecutor {
        return PresetGraphExecutor(
            textApiClient = TextApiClient(OkHttpClient()),
            visionApiClient = VisionApiClient(OkHttpClient()),
            apiKeys = { ApiKeys() },
            runtimeSettings = { PresetRuntimeSettings() },
            uiLanguage = { "en" },
            executionState = state,
        )
    }

    private fun overlayOnlyPreset(id: String): Preset {
        return Preset(
            id = id,
            nameEn = id,
            nameVi = id,
            nameKo = id,
            presetType = PresetType.IMAGE,
            blocks = listOf(inputAdapter().copy(showOverlay = true, renderMode = "markdown")),
        )
    }
}
