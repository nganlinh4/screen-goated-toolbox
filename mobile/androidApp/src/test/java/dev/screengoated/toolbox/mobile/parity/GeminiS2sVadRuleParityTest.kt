package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.service.AdaptiveS2sVadSnapshot
import dev.screengoated.toolbox.mobile.service.FRAME_SAMPLES
import dev.screengoated.toolbox.mobile.service.groupedFirstAudioTimeoutMs
import dev.screengoated.toolbox.mobile.service.groupedHardTimeoutMs
import dev.screengoated.toolbox.mobile.service.isSegmentWorthSending
import dev.screengoated.toolbox.mobile.service.S2sSegment
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Asserts the Android Gemini S2S VAD accept rule (isSegmentWorthSending) and the
 * grouped-timeout formulas produce the same results as the Windows-canonical
 * functions, by running the SAME golden fixtures the Rust side asserts
 * (parity-fixtures/gemini-s2s-vad/accept-rule.json + timeouts.json). One case
 * directly exercises the 0.08 speech-like floor Android had dropped.
 * See .claude/parity/gemini-s2s-vad.md.
 */
class GeminiS2sVadRuleParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun acceptRuleMatchesGoldenFixture() {
        val doc = json.parseToJsonElement(
            Files.readAllBytes(fixturePath("accept-rule.json")).decodeToString(),
        ).jsonObject
        for (caseEl in doc["cases"]!!.jsonArray) {
            val c = caseEl.jsonObject
            val name = c["name"]!!.jsonPrimitive.content
            val frameCount = c["frameCount"]!!.jsonPrimitive.int
            val segment = S2sSegment(
                id = 0L,
                generation = 0L,
                samples = ShortArray(frameCount * FRAME_SAMPLES),
                speechFrames = c["speechFrames"]!!.jsonPrimitive.int,
                peakRms = c["peakRms"]!!.jsonPrimitive.float,
                meanRms = c["meanRms"]!!.jsonPrimitive.float,
                energeticFrames = c["energeticFrames"]!!.jsonPrimitive.int,
                speechLikeFrames = c["speechLikeFrames"]!!.jsonPrimitive.int,
                activeMs = 0L,
            )
            val vad = AdaptiveS2sVadSnapshot(c["strictness"]!!.jsonPrimitive.float)
            assertEquals(
                "case $name",
                c["expectAccept"]!!.jsonPrimitive.boolean,
                isSegmentWorthSending(segment, vad),
            )
        }
    }

    @Test
    fun groupedTimeoutsMatchGoldenFixture() {
        val doc = json.parseToJsonElement(
            Files.readAllBytes(fixturePath("timeouts.json")).decodeToString(),
        ).jsonObject
        for (caseEl in doc["firstAudio"]!!.jsonArray) {
            val c = caseEl.jsonObject
            assertEquals(
                "firstAudio ${c["sourceAudioMs"]} ${c["textUpdates"]}",
                c["expectMs"]!!.jsonPrimitive.long,
                groupedFirstAudioTimeoutMs(
                    c["sourceAudioMs"]!!.jsonPrimitive.long,
                    c["textUpdates"]!!.jsonPrimitive.int,
                ),
            )
        }
        for (caseEl in doc["hard"]!!.jsonArray) {
            val c = caseEl.jsonObject
            assertEquals(
                "hard ${c["sourceAudioMs"]} ${c["finalAttempt"]}",
                c["expectMs"]!!.jsonPrimitive.long,
                groupedHardTimeoutMs(
                    c["sourceAudioMs"]!!.jsonPrimitive.long,
                    c["finalAttempt"]!!.jsonPrimitive.boolean,
                ),
            )
        }
    }

    private fun fixturePath(name: String): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "gemini-s2s-vad", name),
            Paths.get("..", "..", "parity-fixtures", "gemini-s2s-vad", name),
            Paths.get("parity-fixtures", "gemini-s2s-vad", name),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing gemini-s2s-vad fixture $name. Tried: $candidates")
    }
}
