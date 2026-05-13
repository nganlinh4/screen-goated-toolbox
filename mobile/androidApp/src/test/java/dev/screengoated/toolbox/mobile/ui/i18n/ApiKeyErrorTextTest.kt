package dev.screengoated.toolbox.mobile.ui.i18n

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class ApiKeyErrorTextTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun fixtureBackedApiKeyErrorsProduceGlobalToastText() {
        fixtureCases()
            .filter { it.getValue("raw_error").jsonPrimitive.content.contains("API_KEY") }
            .forEach { case ->
                assertTrue(case.getValue("expected_notice").jsonPrimitive.boolean)
                assertEquals(
                    case.getValue("expected_en").jsonPrimitive.content,
                    apiKeyErrorToastText(case.getValue("raw_error").jsonPrimitive.content, "en"),
                )
            }
    }

    @Test
    fun translationGummyMissingKeyUsesLocalizedStartupMessageDirectly() {
        val case = fixtureCases()
            .first { it.getValue("name").jsonPrimitive.content == "translation_gummy_missing_key" }

        assertTrue(case.getValue("expected_notice").jsonPrimitive.boolean)
        assertNotNull(case.getValue("expected_en").jsonPrimitive.content)
    }

    @Test
    fun unrelatedErrorsDoNotProduceApiKeyToastText() {
        assertEquals(null, apiKeyErrorToastText("network failed", "en"))
    }

    private fun fixtureCases() = json
        .parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
        .jsonObject
        .getValue("cases")
        .jsonArray
        .map { it.jsonObject }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "api-key-notifications", "triggers.json"),
            Paths.get("..", "..", "parity-fixtures", "api-key-notifications", "triggers.json"),
            Paths.get("parity-fixtures", "api-key-notifications", "triggers.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing API key notification fixture. Tried: $candidates")
    }
}
