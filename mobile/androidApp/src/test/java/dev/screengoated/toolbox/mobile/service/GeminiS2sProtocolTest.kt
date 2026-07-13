package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class GeminiS2sProtocolTest {
    @Test
    fun `s2s payloads keep canonical live api fields`() {
        val setup = Json.parseToJsonElement(
            buildGeminiS2sSetupPayload(
                GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1,
                settings(),
            ),
        ).jsonObject.getValue("setup").jsonObject
        val generation = setup.getValue("generationConfig").jsonObject

        assertEquals(
            "AUDIO",
            generation.getValue("responseModalities").jsonArray.single().jsonPrimitive.content,
        )
        assertEquals("MEDIA_RESOLUTION_LOW", generation.getValue("mediaResolution").jsonPrimitive.content)
        assertEquals(65536, generation.getValue("maxOutputTokens").jsonPrimitive.content.toInt())
        assertTrue(generation.getValue("thinkingConfig").jsonObject.containsKey("thinkingLevel"))
        assertTrue(setup.containsKey("inputAudioTranscription"))
        assertTrue(setup.containsKey("outputAudioTranscription"))
        assertTrue(setup.containsKey("contextWindowCompression"))
        val audio = Json.parseToJsonElement(
            buildGeminiS2sAudioPayload(shortArrayOf(1, 2)),
        ).jsonObject.getValue("realtimeInput").jsonObject.getValue("audio").jsonObject
        assertEquals("audio/pcm;rate=16000", audio.getValue("mimeType").jsonPrimitive.content)
        val streamEnd = Json.parseToJsonElement(
            buildGeminiS2sAudioStreamEndPayload(),
        ).jsonObject.getValue("realtimeInput").jsonObject
        assertTrue(streamEnd.getValue("audioStreamEnd").jsonPrimitive.content.toBoolean())
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
    fun `s2s sockets preserve setup typestate and check every active send`() {
        val segmentsSource = loadSourceFile(SEGMENTS_SOURCE_PATH).readText()
        val liveTranslateSource = loadSourceFile(LIVE_TRANSLATE_SOURCE_PATH).readText()
        val lifecycleAdapterSource = loadSourceFile(LIVE_LIFECYCLE_ADAPTER_SOURCE_PATH).readText()
        val playbackSource = loadSourceFile(PLAYBACK_SOURCE_PATH).readText()
        val socketSources = "$segmentsSource\n$liveTranslateSource\n$lifecycleAdapterSource"

        assertTrue(segmentsSource.contains("openGeminiLiveReadySession(httpClient, apiKey, setupPayload)"))
        assertTrue(liveTranslateSource.contains("openGeminiLiveConnectedSession(httpClient, apiKey)"))
        assertFalse(liveTranslateSource.contains("openGeminiLiveReadySession("))
        assertTrue(
            lifecycleAdapterSource.contains(
                "is GeminiLiveLifecycleEffect.SendSetup -> activate(effect.generation)",
            ),
        )
        assertTrue(
            lifecycleAdapterSource.contains(
                "pending.session.activate(setupPayload(), policy.setupTimeoutMs)",
            ),
        )
        assertTrue(segmentsSource.contains("if (!session.trySend(buildGeminiS2sAudioPayload("))
        assertTrue(segmentsSource.contains("if (!session.trySend(buildGeminiS2sAudioStreamEndPayload()))"))
        assertTrue(liveTranslateSource.contains("val sent = active.session.trySend(buildGeminiS2sAudioPayload(frame))"))
        assertFalse(socketSources.contains("BlockingWebSocketSession"))
        assertFalse(socketSources.contains("waitForGeminiS2sSetup"))
        assertFalse(playbackSource.contains("waitForGeminiS2sSetup"))
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
    fun `translate model setup uses translation config`() {
        val setup = Json.parseToJsonElement(
            buildGeminiS2sSetupPayload(
                GeneratedLiveModelCatalog.GEMINI_LIVE_TRANSLATE_API_MODEL,
                settings(),
            ),
        ).jsonObject.getValue("setup").jsonObject
        val translation = setup.getValue("generationConfig").jsonObject
            .getValue("translationConfig").jsonObject

        assertEquals("vi", translation.getValue("targetLanguageCode").jsonPrimitive.content)
        assertTrue(translation.getValue("echoTargetLanguage").jsonPrimitive.content.toBoolean())
        assertTrue(setup.containsKey("inputAudioTranscription"))
        assertTrue(setup.containsKey("outputAudioTranscription"))
        assertTrue(isGeminiLiveTranslateApiModel(GeneratedLiveModelCatalog.GEMINI_LIVE_TRANSLATE_API_MODEL))
        assertFalse(isGeminiLiveTranslateApiModel(GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1))
        assertFalse(isGeminiLiveTranslateApiModel("future-unknown-model"))
        assertTrue(shouldSendAudioStreamEnd("future-unknown-model"))
        assertFalse(shouldSendAudioStreamEnd(GeneratedLiveModelCatalog.GEMINI_LIVE_TRANSLATE_API_MODEL))
        assertEquals(
            "RealtimeLiveTranslateAndroid",
            geminiLiveAudioLogTag(GeneratedLiveModelCatalog.GEMINI_LIVE_TRANSLATE_API_MODEL),
        )
        assertEquals("zh-Hans", targetLanguageCode("Chinese"))
        assertEquals("zh-Hant", targetLanguageCode("Chinese (Traditional)"))
        assertEquals("pt-BR", targetLanguageCode("pt-BR"))
        assertEquals("fil", targetLanguageCode("Filipino"))
    }

    private fun settings() = GeminiS2sRuntimeSettings(
        targetLanguage = "Vietnamese",
        customInstruction = "",
        globalTts = MobileGlobalTtsSettings(),
        realtime = RealtimeTtsSettings(),
    )

    @Test
    fun `live translate continuous socket policy matches Windows probes`() {
        val policy = GeminiLiveLifecyclePolicy.continuous()
        val source = loadSourceFile(LIVE_TRANSLATE_SOURCE_PATH).readText()

        assertEquals(100L, policy.serverIdleMinInputChunks)
        assertEquals(15_000L, policy.serverIdleTimeoutMs)
        assertEquals(12 * 60 * 1_000L, policy.rotateAfterMs)
        assertEquals(3_000L, policy.rotationQuietMs)
        assertTrue(source.contains("GeminiS2sLiveLifecycleAdapter("))
        assertTrue(source.contains("clockMs = SystemClock::elapsedRealtime"))
        assertTrue(source.contains("continuous reconnect scheduled reason="))
        assertTrue(source.contains("socket_age_ms="))
        assertTrue(source.contains("since_server_ms="))
        assertTrue(source.contains("since_input_ms="))
        assertTrue(source.contains("reconnect_attempts="))
        assertFalse(source.contains("liveTranslateReconnectDelayMs"))
        assertFalse(source.contains("LIVE_TRANSLATE_SERVER_SILENT_MS"))
        assertFalse(source.contains("LIVE_TRANSLATE_PROACTIVE_ROTATE_MS"))
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
        private const val LIVE_LIFECYCLE_ADAPTER_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/GeminiS2sLiveLifecycleAdapter.kt"
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
