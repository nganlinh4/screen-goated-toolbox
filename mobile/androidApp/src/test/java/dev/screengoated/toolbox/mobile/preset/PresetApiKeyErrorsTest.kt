package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import org.junit.Assert.assertEquals
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
}
