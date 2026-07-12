package dev.screengoated.toolbox.mobile.helpassistant

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class HelpAssistantClientTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun constantsMatchAndroidHelpIndexContract() {
        assertEquals("gemini-3.1-flash-lite", PRIMARY_MODEL)
        assertEquals("gemma-4-26b-a4b-it", FALLBACK_MODEL)
        assertEquals(4096, MAX_OUTPUT_TOKENS)
        assertEquals(
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/help-index.json",
            HELP_INDEX_URL,
        )
    }

    @Test
    fun promptPreservesAndroidHelpAssistantContract() {
        assertTrue(HelpAssistantClient.SYSTEM_PROMPT.contains("Android app help assistant"))
        assertTrue(HelpAssistantClient.SYSTEM_PROMPT.contains("no made up information"))
        assertTrue(HelpAssistantClient.SYSTEM_PROMPT.contains("Do not mention \"Based on the source code\""))
        assertTrue(HelpAssistantClient.SYSTEM_PROMPT.contains("Markdown"))
    }

    @Test
    fun fixtureTracksCurrentHelpIndexContract() {
        val cases = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
            .jsonObject
            .getValue("cases")
            .jsonArray
            .associate { element ->
                val case = element.jsonObject
                case.getValue("name").jsonPrimitive.content to case
            }

        val index = cases.getValue("help_index_context")
        assertEquals(HELP_INDEX_URL, index.getValue("url").jsonPrimitive.content)
        assertEquals(HELP_ASSISTANT_TOP_K, index.getValue("top_k").jsonPrimitive.content.toInt())
        assertTrue(index.getValue("empty_terms_use_first_chunks").jsonPrimitive.boolean)
        assertEquals(listOf("path", "text"), index.getValue("score_inputs").jsonArray.map { it.jsonPrimitive.content })
        assertEquals(3.0, index.getValue("path_match_boost").jsonPrimitive.double, 0.0)

        val modelChain = cases.getValue("model_chain")
        assertEquals(PRIMARY_MODEL, modelChain.getValue("primary").jsonPrimitive.content)
        assertEquals(FALLBACK_MODEL, modelChain.getValue("fallback").jsonPrimitive.content)
        assertEquals(MAX_OUTPUT_TOKENS, modelChain.getValue("max_output_tokens").jsonPrimitive.content.toInt())
        assertEquals(0.7, modelChain.getValue("temperature").jsonPrimitive.double, 0.0)

        val prompt = cases.getValue("prompt_contract")
        assertTrue(prompt.getValue("question_language_answer").jsonPrimitive.boolean)
        assertTrue(prompt.getValue("markdown_output").jsonPrimitive.boolean)
        assertTrue(prompt.getValue("forbid_made_up_information").jsonPrimitive.boolean)
        assertTrue(prompt.getValue("forbid_source_code_framing").jsonPrimitive.boolean)

        val overlayRecovery = cases.getValue("android_overlay_permission_recovery")
        assertEquals(
            "launch_system_overlay_settings",
            overlayRecovery.getValue("missing_overlay_permission").jsonPrimitive.content,
        )
        assertTrue(overlayRecovery.getValue("pending_question_preserved").jsonPrimitive.boolean)
        assertTrue(overlayRecovery.getValue("retry_overlay_after_permission_granted").jsonPrimitive.boolean)
        assertEquals("none", overlayRecovery.getValue("fallback_answer_surface").jsonPrimitive.content)
    }

    @Test
    fun searchRankingUsesWindowsNonOverlappingMatchCount() {
        val case = fixtureCase("search_ranking_uses_non_overlapping_matches")
        val chunksById = case.getValue("chunks").jsonArray.map { element ->
            val chunk = element.jsonObject
            chunk.getValue("id").jsonPrimitive.content to ChunkEntry(
                path = chunk.getValue("path").jsonPrimitive.content,
                text = chunk.getValue("text").jsonPrimitive.content,
            )
        }
        val ranked = rankHelpAssistantChunks(
            index = chunksById.map { it.second },
            question = case.getValue("question").jsonPrimitive.content,
        )
        val idsByChunk = chunksById.associate { (id, chunk) -> chunk to id }
        val expected = case.getValue("expected_top_ids").jsonArray.map { it.jsonPrimitive.content }

        assertEquals(expected, ranked.map { idsByChunk.getValue(it) })
    }

    @Test
    fun localizedPlaceholderAndLoadingTextUseAndroidScopedCopy() {
        val locale = MobileLocaleText.forLanguage("en")

        assertEquals(locale.helpAssistantRestPlaceholder, helpPlaceholder(locale))
        assertEquals(locale.helpAssistantRestLoading, helpLoadingMessage(locale))
    }

    @Test
    fun resultMarkdownUsesQuestionAndAnswer() {
        val markdown = helpResultMarkdown(
            question = "How do I use the bubble?",
            answer = "Tap it.",
        )

        assertTrue(markdown.startsWith("### How do I use the bubble?"))
        assertTrue(markdown.endsWith("Tap it."))
    }

    @Test
    fun errorMarkdownIsUserVisible() {
        val markdown = helpErrorMarkdown("Missing key")

        assertTrue(markdown.contains("## ❌ Error"))
        assertTrue(markdown.contains("Missing key"))
    }

    @Test
    fun missingKeyUsesSharedApiKeyNoticeMarker() = runTest {
        val result = HelpAssistantClient(okhttp3.OkHttpClient()).ask(
            HelpAssistantRequest(
                question = "How do I use it?",
                uiLanguage = "en",
                geminiApiKey = "",
            ),
        )

        assertTrue(result.isFailure)
        assertEquals("NO_API_KEY:gemini", result.exceptionOrNull()?.message)
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "mobile-shell", "help-assistant.json"),
            Paths.get("..", "..", "parity-fixtures", "mobile-shell", "help-assistant.json"),
            Paths.get("parity-fixtures", "mobile-shell", "help-assistant.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing help assistant fixture. Tried: $candidates")
    }

    private fun fixtureCase(name: String) = json
        .parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
        .jsonObject
        .getValue("cases")
        .jsonArray
        .first { it.jsonObject.getValue("name").jsonPrimitive.content == name }
        .jsonObject
}
