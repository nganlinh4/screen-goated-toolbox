package dev.screengoated.toolbox.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
        val vadSource = loadSourceFile(VAD_SOURCE_PATH).readText()

        assertTrue(vadSource.contains("internal const val HEDGE_ATTEMPTS = 2"))
        assertTrue(vadSource.contains("internal const val FIRST_AUDIO_SILENT_RETRY_MS = 3_800L"))
        assertTrue(vadSource.contains("internal const val FIRST_AUDIO_ACTIVE_RETRY_MS = 5_200L"))
        assertTrue(source.contains("hedge-winner"))
        assertTrue(source.contains("reason=no_first_audio_retry"))
    }

    @Test
    fun `s2s playback preserves full committed display text`() {
        val source = loadSourceFile(PLAYBACK_SOURCE_PATH).readText()

        assertTrue(source.contains("sourceCommitted = sourceCommitted"))
        assertTrue(source.contains("targetCommitted = targetCommitted"))
        assertFalse(source.contains("RECENT_DISPLAY_CHARS"))
        assertFalse(source.contains("recentGeminiS2sWindow"))
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

    @Test
    fun `translate model setup source uses translation config`() {
        val source = loadSourceFile(PROTOCOL_SOURCE_PATH).readText()
        val clientSource = loadSourceFile(CLIENT_SOURCE_PATH).readText()

        assertTrue(source.contains("isGeminiTranslateApiModel(model)"))
        assertTrue(source.contains("RealtimeModelIds.GEMINI_LIVE_TRANSLATE_API_MODEL"))
        assertTrue(source.contains("\"translationConfig\""))
        assertTrue(source.contains(".put(\"targetLanguageCode\", targetLanguageCode(settings.targetLanguage))"))
        assertTrue(source.contains(".put(\"echoTargetLanguage\", true)"))
        assertTrue(source.contains("\"translationConfig\",\n                                JSONObject()"))
        assertTrue(source.contains(".put(\"inputAudioTranscription\", JSONObject())"))
        assertTrue(source.contains(".put(\"outputAudioTranscription\", JSONObject()),"))
        assertTrue(clientSource.contains("shouldSendAudioStreamEnd(model)"))
        assertTrue(clientSource.contains("stream-end-skipped"))
        assertTrue(clientSource.contains("RealtimeLiveTranslateAndroid"))
        assertTrue(clientSource.contains("runLiveTranslateContinuousSession("))
        assertTrue(clientSource.contains("collectSegments(audioChunks, adaptiveVad, backlogMs, logTag)"))
        assertTrue(clientSource.contains("player.playNativePcm24k(bytes, volumePercent)"))
        assertEquals("zh-Hans", targetLanguageCode("Chinese"))
        assertEquals("zh-Hant", targetLanguageCode("Chinese (Traditional)"))
        assertEquals("pt-BR", targetLanguageCode("pt-BR"))
        assertEquals("fil", targetLanguageCode("Filipino"))
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
        private const val PLAYBACK_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sPlayback.kt"
        private const val PROTOCOL_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sProtocol.kt"
        private const val VAD_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sVad.kt"
    }
}
