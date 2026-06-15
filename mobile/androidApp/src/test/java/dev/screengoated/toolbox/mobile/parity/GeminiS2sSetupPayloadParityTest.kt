package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.service.GeminiS2sRuntimeSettings
import dev.screengoated.toolbox.mobile.service.buildGeminiS2sSetupPayload
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Asserts the Android Gemini live-translate setup payload (buildGeminiS2sSetupPayload,
 * isGeminiTranslateApiModel branch) is structurally identical to the Windows-canonical
 * build_live_translate_setup_value, via the shared fixture
 * (parity-fixtures/gemini-s2s-setup/live-translate.json) the Rust side asserts too.
 * Compared as parsed JSON (field order ignored). The legacy interpreter payload's
 * instruction prose differs by design and is NOT locked. See .claude/parity/gemini-s2s-vad.md.
 */
class GeminiS2sSetupPayloadParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun liveTranslateSetupMatchesGoldenFixture() {
        val doc = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        val input = doc["input"]!!.jsonObject
        val expect = doc["expect"]!!.jsonObject

        val settings = GeminiS2sRuntimeSettings(
            targetLanguage = input["targetLanguage"]!!.jsonPrimitive.content,
            customInstruction = "",
            globalTts = MobileGlobalTtsSettings(),
            realtime = RealtimeTtsSettings(),
        )
        val payloadStr =
            buildGeminiS2sSetupPayload(input["model"]!!.jsonPrimitive.content, settings)
        val actual = json.parseToJsonElement(payloadStr).jsonObject

        assertEquals(expect, actual)
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "gemini-s2s-setup", "live-translate.json"),
            Paths.get("..", "..", "parity-fixtures", "gemini-s2s-setup", "live-translate.json"),
            Paths.get("parity-fixtures", "gemini-s2s-setup", "live-translate.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing gemini-s2s-setup fixture. Tried: $candidates")
    }
}
