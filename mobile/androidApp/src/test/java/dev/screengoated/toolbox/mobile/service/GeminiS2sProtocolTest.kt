package dev.screengoated.toolbox.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class GeminiS2sProtocolTest {
    @Test
    fun `s2s source keeps canonical live api setup fields`() {
        val source = loadSourceFile(PROTOCOL_SOURCE_PATH).readText()

        assertTrue(source.contains(".put(\"responseModalities\", JSONArray().put(\"AUDIO\"))"))
        assertTrue(source.contains(".put(\"mediaResolution\", \"MEDIA_RESOLUTION_LOW\")"))
        assertTrue(source.contains(".put(\"thinkingConfig\", JSONObject().put(\"thinkingBudget\", 0))"))
        assertTrue(source.contains(".put(\"inputAudioTranscription\", JSONObject())"))
        assertTrue(source.contains(".put(\"outputAudioTranscription\", JSONObject())"))
        assertTrue(source.contains(".put(\"contextWindowCompression\", JSONObject().put(\"slidingWindow\", JSONObject()))"))
        assertTrue(source.contains(".put(\"audioStreamEnd\", true)"))
        assertTrue(source.contains(".put(\"mimeType\", \"audio/pcm;rate=16000\")"))
    }

    @Test
    fun `s2s source implements hedged attempts and first audio retry`() {
        val source = loadSourceFile(CLIENT_SOURCE_PATH).readText()

        assertTrue(source.contains("private const val HEDGE_ATTEMPTS = 2"))
        assertTrue(source.contains("private const val FIRST_AUDIO_SILENT_RETRY_MS = 3_800L"))
        assertTrue(source.contains("private const val FIRST_AUDIO_ACTIVE_RETRY_MS = 5_200L"))
        assertTrue(source.contains("hedge-winner"))
        assertTrue(source.contains("reason=no_first_audio_retry"))
    }

    @Test
    fun `segment text merge preserves word boundaries after overlap trim`() {
        assertEquals(
            "alpha beta gamma delta",
            mergeGeminiS2sSegmentText("alpha beta gamma", "beta gamma delta"),
        )
        assertEquals(
            "alpha beta",
            mergeGeminiS2sSegmentText("alpha", "beta"),
        )
    }

    private fun loadSourceFile(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, path) }
            .firstOrNull(File::exists)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private companion object {
        private const val CLIENT_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sClient.kt"
        private const val PROTOCOL_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sProtocol.kt"
    }
}
