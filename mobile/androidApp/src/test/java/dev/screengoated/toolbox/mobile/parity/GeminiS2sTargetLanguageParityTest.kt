package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.service.targetLanguageCode
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
 * Asserts the Android Gemini S2S target-language BCP-47 mapping's explicit special
 * cases match the Windows-canonical live_translate_target_language_code, via the
 * shared fixture (parity-fixtures/gemini-s2s-language/target-language-codes.json)
 * the Rust side asserts too. Only the explicit special cases are locked — the
 * general name->code fallback (isolang vs LanguageCatalog) is a documented
 * deviation. See .claude/parity/gemini-s2s-vad.md.
 */
class GeminiS2sTargetLanguageParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun specialCasesMatchGoldenFixture() {
        val doc = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        for (caseEl in doc["cases"]!!.jsonArray) {
            val c = caseEl.jsonObject
            val input = c["input"]!!.jsonPrimitive.content
            assertEquals(
                "input \"$input\"",
                c["expect"]!!.jsonPrimitive.content,
                targetLanguageCode(input),
            )
        }
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "gemini-s2s-language", "target-language-codes.json"),
            Paths.get("..", "..", "parity-fixtures", "gemini-s2s-language", "target-language-codes.json"),
            Paths.get("parity-fixtures", "gemini-s2s-language", "target-language-codes.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing gemini-s2s-language fixture. Tried: $candidates")
    }
}
