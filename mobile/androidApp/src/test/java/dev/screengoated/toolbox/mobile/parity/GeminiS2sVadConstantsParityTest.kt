package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.service.ABSOLUTE_SPEECH_RMS
import dev.screengoated.toolbox.mobile.service.AUDIO_IDLE_FINISH_MS
import dev.screengoated.toolbox.mobile.service.END_SILENCE_FRAMES
import dev.screengoated.toolbox.mobile.service.FIRST_AUDIO_ACTIVE_RETRY_MS
import dev.screengoated.toolbox.mobile.service.FIRST_AUDIO_SILENT_RETRY_MS
import dev.screengoated.toolbox.mobile.service.FRAME_SAMPLES
import dev.screengoated.toolbox.mobile.service.MAX_SEGMENT_SAMPLES
import dev.screengoated.toolbox.mobile.service.MAX_SPEECH_THRESHOLD
import dev.screengoated.toolbox.mobile.service.MIN_SEGMENT_PEAK_RMS
import dev.screengoated.toolbox.mobile.service.MIN_SEGMENT_SAMPLES
import dev.screengoated.toolbox.mobile.service.MIN_SEGMENT_SPEECH_FRAMES
import dev.screengoated.toolbox.mobile.service.MIN_SEGMENT_SPEECH_RATIO
import dev.screengoated.toolbox.mobile.service.MIN_SPEECH_LIKE_RATIO
import dev.screengoated.toolbox.mobile.service.MIN_SPEECH_THRESHOLD
import dev.screengoated.toolbox.mobile.service.NOISE_LEARN_MAX_RMS
import dev.screengoated.toolbox.mobile.service.NOISE_LEARN_THRESHOLD_RATIO
import dev.screengoated.toolbox.mobile.service.PREROLL_SAMPLES
import dev.screengoated.toolbox.mobile.service.S2S_HEDGE_FINAL_TIMEOUT_MS
import dev.screengoated.toolbox.mobile.service.S2S_HEDGE_TIMEOUT_MS
import dev.screengoated.toolbox.mobile.service.SESSION_COUNT
import dev.screengoated.toolbox.mobile.service.SPEECH_THRESHOLD_MULTIPLIER
import dev.screengoated.toolbox.mobile.service.STRICT_MIN_SPEECH_CONFIDENCE
import dev.screengoated.toolbox.mobile.service.STRICT_MIN_SPEECH_LIKE_RATIO
import dev.screengoated.toolbox.mobile.service.TARGET_SEGMENT_SAMPLES
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlin.math.abs

/**
 * Locks the Android Gemini S2S VAD/segmentation/timeout constants against the
 * Windows-canonical values via the shared fixture
 * (`parity-fixtures/gemini-s2s-vad/constants.json`), which the Rust side asserts
 * too. If the duplicated tuning drifts on either platform, one suite goes red.
 * See .claude/parity/gemini-s2s-vad.md.
 */
class GeminiS2sVadConstantsParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun matchesSharedConstantsFixture() {
        val doc =
            json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        val ints = doc["ints"]!!.jsonObject
        val floats = doc["floats"]!!.jsonObject

        val intConsts: Map<String, Long> = mapOf(
            "FRAME_SAMPLES" to FRAME_SAMPLES.toLong(),
            "PREROLL_SAMPLES" to PREROLL_SAMPLES.toLong(),
            "MIN_SEGMENT_SAMPLES" to MIN_SEGMENT_SAMPLES.toLong(),
            "TARGET_SEGMENT_SAMPLES" to TARGET_SEGMENT_SAMPLES.toLong(),
            "MAX_SEGMENT_SAMPLES" to MAX_SEGMENT_SAMPLES.toLong(),
            "END_SILENCE_FRAMES" to END_SILENCE_FRAMES.toLong(),
            "SESSION_COUNT" to SESSION_COUNT.toLong(),
            "MIN_SEGMENT_SPEECH_FRAMES" to MIN_SEGMENT_SPEECH_FRAMES.toLong(),
            "FIRST_AUDIO_SILENT_RETRY_MS" to FIRST_AUDIO_SILENT_RETRY_MS,
            "FIRST_AUDIO_ACTIVE_RETRY_MS" to FIRST_AUDIO_ACTIVE_RETRY_MS,
            "AUDIO_IDLE_FINISH_MS" to AUDIO_IDLE_FINISH_MS,
            "S2S_HEDGE_TIMEOUT_MS" to S2S_HEDGE_TIMEOUT_MS,
            "S2S_HEDGE_FINAL_TIMEOUT_MS" to S2S_HEDGE_FINAL_TIMEOUT_MS,
        )
        for ((name, value) in intConsts) {
            assertEquals("int $name", ints[name]!!.jsonPrimitive.long, value)
        }
        assertEquals("int count", ints.size, intConsts.size)

        val floatConsts: Map<String, Float> = mapOf(
            "SPEECH_THRESHOLD_MULTIPLIER" to SPEECH_THRESHOLD_MULTIPLIER,
            "MIN_SPEECH_THRESHOLD" to MIN_SPEECH_THRESHOLD,
            "MAX_SPEECH_THRESHOLD" to MAX_SPEECH_THRESHOLD,
            "ABSOLUTE_SPEECH_RMS" to ABSOLUTE_SPEECH_RMS,
            "NOISE_LEARN_MAX_RMS" to NOISE_LEARN_MAX_RMS,
            "NOISE_LEARN_THRESHOLD_RATIO" to NOISE_LEARN_THRESHOLD_RATIO,
            "MIN_SEGMENT_PEAK_RMS" to MIN_SEGMENT_PEAK_RMS,
            "MIN_SEGMENT_SPEECH_RATIO" to MIN_SEGMENT_SPEECH_RATIO,
            "MIN_SPEECH_LIKE_RATIO" to MIN_SPEECH_LIKE_RATIO,
            "STRICT_MIN_SPEECH_LIKE_RATIO" to STRICT_MIN_SPEECH_LIKE_RATIO,
            "STRICT_MIN_SPEECH_CONFIDENCE" to STRICT_MIN_SPEECH_CONFIDENCE,
        )
        for ((name, value) in floatConsts) {
            assertTrue(
                "float $name",
                abs(floats[name]!!.jsonPrimitive.double - value.toDouble()) < 1e-6,
            )
        }
        assertEquals("float count", floats.size, floatConsts.size)
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "gemini-s2s-vad", "constants.json"),
            Paths.get("..", "..", "parity-fixtures", "gemini-s2s-vad", "constants.json"),
            Paths.get("parity-fixtures", "gemini-s2s-vad", "constants.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing gemini-s2s-vad fixture. Tried: $candidates")
    }
}
