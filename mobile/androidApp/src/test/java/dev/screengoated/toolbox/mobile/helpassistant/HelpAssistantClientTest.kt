package dev.screengoated.toolbox.mobile.helpassistant

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HelpAssistantClientTest {
    private val client = HelpAssistantClient(OkHttpClient())

    @Test
    fun bucketOrderAndRawFilesMatchParityContract() {
        assertEquals(
            listOf("screen-record", "android", "rest"),
            HelpAssistantBucket.entries.map { it.wireId },
        )
        assertEquals(
            listOf(
                "repomix-screen-recorder.xml",
                "repomix-android.xml",
                "repomix-rest.xml",
            ),
            HelpAssistantBucket.entries.map { it.rawFileName },
        )
    }

    @Test
    fun rawUrlsPointToTrackedGithubFiles() {
        assertEquals(
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-screen-recorder.xml",
            HelpAssistantBucket.SCREEN_RECORD.rawUrl(),
        )
        assertEquals(
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-android.xml",
            HelpAssistantBucket.ANDROID.rawUrl(),
        )
        assertEquals(
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/repomix-rest.xml",
            HelpAssistantBucket.REST.rawUrl(),
        )
    }

    @Test
    fun userMessagePreservesCanonicalPromptContract() {
        val text = client.buildUserMessage(
            mode = HelpAssistantMode.QUICK,
            question = "How do I use this?",
            contextXml = "<repo />",
        )

        assertTrue(text.contains(HelpAssistantClient.SYSTEM_PROMPT))
        assertTrue(text.contains(HelpAssistantMode.QUICK.promptInstruction))
        assertTrue(text.contains("Source Code Context:\n<repo />"))
        assertTrue(text.contains("User Question: How do I use this?"))
    }

    @Test
    fun answerModesMapToExpectedGeminiModels() {
        assertEquals("gemini-3.1-flash-lite-preview", HelpAssistantMode.QUICK.modelId)
        assertEquals("gemini-3-flash-preview", HelpAssistantMode.DETAILED.modelId)
        assertEquals(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite-preview:generateContent",
            HelpAssistantClient.geminiEndpoint(HelpAssistantMode.QUICK),
        )
        assertEquals(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-flash-preview:generateContent",
            HelpAssistantClient.geminiEndpoint(HelpAssistantMode.DETAILED),
        )
    }

    @Test
    fun resultMarkdownUsesBucketLabelAndQuestion() {
        val locale = MobileLocaleText.forLanguage("en")
        val markdown = HelpAssistantBucket.ANDROID.resultMarkdown(
            locale = locale,
            question = "How do I use the bubble?",
            answer = "Tap it.",
        )

        assertTrue(markdown.contains("## \uD83D\uDCF1 Ask about SGT Android"))
        assertTrue(markdown.contains("### How do I use the bubble?"))
        assertTrue(markdown.endsWith("Tap it."))
    }
}
