package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.model.MobileTtsCatalog
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Cross-platform parity guard for the Gemini voice + instruction-language catalog.
 *
 * Windows (Rust, `src/config/tts_catalog_gemini.rs`) is canonical. The same shared
 * fixture is asserted by the Rust test in that file, so the Android catalog cannot
 * silently drift from Windows. Kokoro/Supertonic voices are intentionally excluded on
 * Android (Gemini only). See `.claude/parity/gemini-voice-catalog.md`.
 */
class GeminiVoiceCatalogParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `gemini voices match shared parity fixture`() {
        val fixtureVoices = catalog()
            .getValue("voices")
            .jsonArray
            .map { entry ->
                val obj = entry.jsonObject
                obj.getValue("name").jsonPrimitive.content to obj.getValue("gender").jsonPrimitive.content
            }
            .toSet()

        // Android splits the canonical (name, gender) list into male/female lists;
        // reconstruct the full set and compare by name + gender.
        val androidVoices = (MobileTtsCatalog.maleVoices + MobileTtsCatalog.femaleVoices)
            .map { it.name to it.gender }
            .toSet()

        assertEquals(
            "Android Gemini voice catalog drifted from the canonical Rust fixture",
            fixtureVoices,
            androidVoices,
        )
        assertEquals(
            "Android Gemini voice count drifted from the canonical Rust fixture",
            fixtureVoices.size,
            MobileTtsCatalog.maleVoices.size + MobileTtsCatalog.femaleVoices.size,
        )
    }

    @Test
    fun `gemini instruction languages match shared parity fixture`() {
        val fixtureLanguages = catalog()
            .getValue("instructionLanguages")
            .jsonArray
            .map { entry ->
                val obj = entry.jsonObject
                obj.getValue("code").jsonPrimitive.content to obj.getValue("name").jsonPrimitive.content
            }

        val androidLanguages = MobileTtsCatalog.conditionLanguages.map { it.code to it.name }

        // Exact order parity with Rust SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES.
        assertEquals(
            "Android Gemini instruction-language catalog drifted from the canonical Rust fixture",
            fixtureLanguages,
            androidLanguages,
        )
    }

    private fun catalog() = json
        .parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
        .jsonObject

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "gemini-voice-catalog", "catalog.json"),
            Paths.get("..", "..", "parity-fixtures", "gemini-voice-catalog", "catalog.json"),
            Paths.get("parity-fixtures", "gemini-voice-catalog", "catalog.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing Gemini voice catalog fixture. Tried: $candidates")
    }
}
