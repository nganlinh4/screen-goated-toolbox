package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.translationgummy.TranslationGummyRuntime
import dev.screengoated.toolbox.mobile.translationgummy.buildTranslationGummySetupPayload
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Cross-platform parity lock for the Gemini Live Translate (Translation Gummy)
 * VAD numerics and session setup-payload constants.
 *
 * Rust is canonical (src/overlay/translation_gummy/runtime.rs). This test loads the
 * same shared fixture the Rust test asserts against so the two cannot drift.
 * See .claude/parity/translation-gummy.md.
 *
 * Pre-roll note: Windows expresses pre-roll in samples (3200 = 2 * 1600). The Android
 * audio chunk size is device-dependent, so the contract is the chunk count (2). Android
 * asserts LOCAL_INPUT_PREROLL_CHUNKS == fixture.vad.prerollChunks.
 */
class TranslationGummyVadContractTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `vad constants match parity fixture`() {
        val vad = loadFixture().getValue("vad").jsonObject

        assertEquals(
            vad.getValue("speechRms").jsonPrimitive.content.toFloat(),
            TranslationGummyRuntime.LOCAL_INPUT_SPEECH_RMS,
            0.0f,
        )
        assertEquals(
            vad.getValue("trailingAudioMs").jsonPrimitive.content.toLong(),
            TranslationGummyRuntime.LOCAL_INPUT_TRAILING_AUDIO_MS,
        )
        assertEquals(
            vad.getValue("endSilenceMs").jsonPrimitive.content.toLong(),
            TranslationGummyRuntime.LOCAL_INPUT_END_SILENCE_MS,
        )
        assertEquals(
            vad.getValue("prerollChunks").jsonPrimitive.int,
            TranslationGummyRuntime.LOCAL_INPUT_PREROLL_CHUNKS,
        )
    }

    @Test
    fun `setup payload matches parity fixture`() {
        val setupFixture = loadFixture().getValue("setup").jsonObject
        val payload = json.parseToJsonElement(
            buildTranslationGummySetupPayload(
                model = "model-x",
                instruction = "instruction",
                voiceName = "VoiceX",
            ),
        ).jsonObject
        val setup = payload.getValue("setup").jsonObject
        val generation = setup.getValue("generationConfig").jsonObject
        val realtime = setup.getValue("realtimeInputConfig").jsonObject
        val activity = realtime.getValue("automaticActivityDetection").jsonObject

        assertEquals(
            setupFixture.getValue("startSensitivity").jsonPrimitive.content,
            activity.getValue("startOfSpeechSensitivity").jsonPrimitive.content,
        )
        assertEquals(
            setupFixture.getValue("endSensitivity").jsonPrimitive.content,
            activity.getValue("endOfSpeechSensitivity").jsonPrimitive.content,
        )
        assertEquals(
            setupFixture.getValue("prefixPaddingMs").jsonPrimitive.int,
            activity.getValue("prefixPaddingMs").jsonPrimitive.int,
        )
        assertEquals(
            setupFixture.getValue("silenceDurationMs").jsonPrimitive.int,
            activity.getValue("silenceDurationMs").jsonPrimitive.int,
        )
        assertEquals(
            setupFixture.getValue("thinkingBudget").jsonPrimitive.int,
            generation.getValue("thinkingConfig").jsonObject.getValue("thinkingBudget").jsonPrimitive.int,
        )
        assertEquals(
            setupFixture.getValue("mediaResolution").jsonPrimitive.content,
            generation.getValue("mediaResolution").jsonPrimitive.content,
        )
        assertEquals(
            setupFixture.getValue("activityHandling").jsonPrimitive.content,
            realtime.getValue("activityHandling").jsonPrimitive.content,
        )
        assertEquals(
            setupFixture.getValue("turnCoverage").jsonPrimitive.content,
            realtime.getValue("turnCoverage").jsonPrimitive.content,
        )
    }

    private fun loadFixture(): JsonObject {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")

        return json.parseToJsonElement(File(repoRoot, FIXTURE_PATH).readText()).jsonObject
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/translation-gummy/vad-contract.json"
    }
}
