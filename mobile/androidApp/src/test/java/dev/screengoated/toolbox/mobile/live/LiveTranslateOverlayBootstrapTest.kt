package dev.screengoated.toolbox.mobile.live

import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.model.RealtimePaneFontSizes
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
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
        val controls = loadFixture().requiredControls
        val translation = listOf(
            RealtimeModelIds.TRANSLATION_CEREBRAS,
            RealtimeModelIds.TRANSLATION_GEMMA,
            RealtimeModelIds.TRANSLATION_GTX,
        )
        val transcription = listOf(
            RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5,
            RealtimeModelIds.TRANSCRIPTION_PARAKEET,
        )

        assertTrue(translation.contains(defaultTranslationProviderId()))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_PARAKEET))
        assertTrue(controls.translationPane.contains("translation-model-toggle"))
        assertTrue(controls.transcriptionPane.contains("transcription-model-toggle"))
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
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")

        return json.decodeFromString(File(repoRoot, FIXTURE_PATH).readText())
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/live-translate/overlay-bootstrap.json"
    }
}

@Serializable
private data class OverlayFixture(
    val defaults: OverlayDefaults,
    val requiredControls: RequiredControls,
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
private data class RequiredControls(
    val transcriptionPane: List<String>,
    val translationPane: List<String>,
)
