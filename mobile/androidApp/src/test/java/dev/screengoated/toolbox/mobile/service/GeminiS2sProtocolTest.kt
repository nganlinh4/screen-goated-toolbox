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
        val source = clientImplementationSource()
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
        val clientSource = clientImplementationSource()

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
        assertTrue(clientSource.contains("player.playNativePcm24k("))
        assertTrue(clientSource.contains("AudioTrackOutputMode.MEDIA"))
        assertTrue(clientSource.contains("AudioTrackOutputMode.VOICE_COMMUNICATION"))
        assertTrue(clientSource.contains("sourceMode == SourceMode.MIC"))
        assertFalse(clientSource.contains("shouldDropLiveTranslateEchoFrame"))
        assertEquals("zh-Hans", targetLanguageCode("Chinese"))
        assertEquals("zh-Hant", targetLanguageCode("Chinese (Traditional)"))
        assertEquals("pt-BR", targetLanguageCode("pt-BR"))
        assertEquals("fil", targetLanguageCode("Filipino"))
    }

    @Test
    fun `live translate continuous socket policy matches Windows probes`() {
        val source = clientImplementationSource()

        assertTrue(source.contains("private const val LIVE_TRANSLATE_SERVER_SILENT_SENT_CHUNKS = 100"))
        assertTrue(source.contains("private const val LIVE_TRANSLATE_SERVER_SILENT_MS = 15_000L"))
        assertTrue(source.contains("private const val LIVE_TRANSLATE_PROACTIVE_ROTATE_MS = 12 * 60 * 1_000L"))
        assertTrue(source.contains("private const val LIVE_TRANSLATE_ROTATE_QUIET_MS = 3_000L"))
        assertTrue(source.contains("private fun liveTranslateReconnectDelayMs("))
        assertTrue(source.contains("val baseMs = 250L * (1L shl cappedAttempt)"))
        assertTrue(source.contains("coerceAtMost(6_000L)"))
        assertTrue(source.contains("continuous reconnect scheduled reason="))
        assertTrue(source.contains("continuous reconnect reason=server-silent"))
        assertTrue(source.contains("continuous reconnect reason=proactive-rotation"))
        assertTrue(source.contains("socket_age_ms="))
        assertTrue(source.contains("since_server_ms="))
        assertTrue(source.contains("since_input_ms="))
        assertTrue(source.contains("reconnect_attempts="))
    }

    /**
     * The S2S client implementation was split across [GeminiS2sClient], [GeminiS2sLiveTranslate],
     * and [GeminiS2sSegments]. The parity contract is "the S2S client implements X", so the
     * assertions read all three implementation files combined to find the code wherever it lives.
     */
    private fun clientImplementationSource(): String {
        return CLIENT_IMPLEMENTATION_PATHS.joinToString(separator = "\n") { path ->
            loadSourceFile(path).readText()
        }
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
        private const val LIVE_TRANSLATE_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sLiveTranslate.kt"
        private const val SEGMENTS_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sSegments.kt"
        private const val PLAYBACK_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sPlayback.kt"
        private const val PROTOCOL_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sProtocol.kt"
        private const val VAD_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sVad.kt"
        private val CLIENT_IMPLEMENTATION_PATHS = listOf(
            CLIENT_SOURCE_PATH,
            LIVE_TRANSLATE_SOURCE_PATH,
            SEGMENTS_SOURCE_PATH,
        )
    }
}
