package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetApiKeyErrorsTest {
    @Test
    fun invalidKeyMessagesPreserveProviderForGlobalToastText() {
        assertEquals("INVALID_API_KEY:openrouter", invalidApiKeyMessage("OpenRouter"))
        assertEquals(
            "Invalid OpenRouter API key!",
            apiKeyErrorToastText(invalidApiKeyMessage("OpenRouter"), "en"),
        )
        assertEquals(
            "Invalid Google Gemini API key!",
            apiKeyErrorToastText(invalidApiKeyMessage("google"), "en"),
        )
        assertEquals(
            "Invalid Groq API key!",
            apiKeyErrorToastText(invalidApiKeyMessage("groq"), "en"),
        )
        assertEquals(
            "Invalid Cerebras API key!",
            apiKeyErrorToastText(invalidApiKeyMessage("cerebras"), "en"),
        )
    }

    @Test
    fun presetInputVoiceMissingKeyFailureUsesGlobalToastBus() {
        val case = loadFixtureCase("preset_input_voice_missing_key")
        assertEquals("NO_API_KEY:google", case.getValue("raw_error").jsonPrimitive.content)
        assertEquals("preset_input_voice", case.getValue("surface").jsonPrimitive.content)

        val source = File(repoRoot(), PRESET_OVERLAY_CONTROLLER_SOURCE).readText()
        assertTrue(source.contains("apiKeyErrorToastText(error.message ?: error.toString(), uiLanguage())"))
        assertTrue(source.contains("?.let(appContainer.toastBus::show)"))
    }

    private fun loadFixtureCase(name: String) =
        Json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText())
            .jsonObject
            .getValue("cases")
            .jsonArray
            .map { it.jsonObject }
            .firstOrNull { it.getValue("name").jsonPrimitive.content == name }
            ?: error("Missing API-key notification fixture case: $name")

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/api-key-notifications/triggers.json"
        private const val PRESET_OVERLAY_CONTROLLER_SOURCE =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetOverlayController.kt"
    }
}
