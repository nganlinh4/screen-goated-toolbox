package dev.screengoated.toolbox.mobile.service.tts

import android.os.SystemClock
import android.util.Base64
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.URLEncoder
import java.nio.charset.StandardCharsets
import java.util.UUID
import java.util.concurrent.LinkedBlockingDeque

internal sealed interface ProviderAudioEvent {
    data class PcmData(
        val payload: ByteArray,
    ) : ProviderAudioEvent

    data class Error(
        val message: String,
    ) : ProviderAudioEvent

    data object End : ProviderAudioEvent
}

internal class GeminiTtsProvider(
    private val httpClient: OkHttpClient,
    private val languageDetector: DeviceLanguageDetector,
) {
    // ── Warm socket pool (pre-connected + pre-setup) ──────────────────────
    private var warmSession: BlockingWebSocketSession? = null
    private var warmApiKey: String? = null
    private var warmCreatedAt: Long = 0
    private val warmLock = Any()
    private val WARM_SOCKET_MAX_AGE_MS = Long.MAX_VALUE // keep forever — re-warm on failure

    /** Pre-connect a WebSocket in the background so next TTS request is instant.
     *  Also starts a keepalive loop that re-warms every 50 seconds. */
    fun warmUp(apiKey: String) {
        if (apiKey.isBlank()) return
        android.util.Log.d("TTS-Timing", "warmUp: starting background connect...")
        Thread {
            try {
                val t0 = SystemClock.elapsedRealtime()
                val req = Request.Builder().url("$LIVE_WS_ENDPOINT?key=$apiKey").build()
                val session = BlockingWebSocketSession(httpClient, req)
                if (!session.awaitOpen(10_000)) { session.close(); return@Thread }
                synchronized(warmLock) {
                    warmSession?.close()
                    warmSession = session
                    warmApiKey = apiKey
                    warmCreatedAt = SystemClock.elapsedRealtime()
                }
                android.util.Log.d("TTS-Timing", "Warm socket ready in ${SystemClock.elapsedRealtime() - t0}ms")
            } catch (e: Exception) {
                android.util.Log.d("TTS-Timing", "Warm-up failed: ${e.message}")
            }
        }.start()
    }

    /** Try to acquire a pre-connected session. Returns null if none available. */
    private fun acquireWarmSession(apiKey: String): BlockingWebSocketSession? {
        synchronized(warmLock) {
            val session = warmSession ?: return null
            if (warmApiKey != apiKey) { session.close(); warmSession = null; return null }
            val age = SystemClock.elapsedRealtime() - warmCreatedAt
            if (age > WARM_SOCKET_MAX_AGE_MS) { session.close(); warmSession = null; return null }
            warmSession = null
            warmApiKey = null
            return session
        }
    }

    fun stream(
        apiKey: String,
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        if (apiKey.isBlank()) {
            sink.offer(ProviderAudioEvent.Error("Add your Gemini API key before using Gemini TTS."))
            return
        }

        val t0 = SystemClock.elapsedRealtime()
        android.util.Log.d("TTS-Timing", "▶ START request: '${request.text.take(30)}...'")

        // Try warm socket first (skips connect time only; setup stays request-specific)
        val warm = acquireWarmSession(apiKey)
        android.util.Log.d("TTS-Timing", "  Warm socket check: ${if (warm != null) "AVAILABLE" else "not available"}")
        if (warm != null) {
            android.util.Log.d("TTS-Timing", "  Using WARM socket (connect already done)")
            streamWithSession(warm, request, isStale, sink, t0)
            // warmUp is called by the worker loop after stream() returns
            return
        }

        val socketRequest = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()

        BlockingWebSocketSession(httpClient, socketRequest).use { session ->
            val t1 = SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  WebSocket object created: ${t1 - t0}ms")

            if (!session.awaitOpen(10_000)) {
                sink.offer(ProviderAudioEvent.Error("Gemini TTS websocket failed to open."))
                return
            }
            val t2 = SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  WebSocket OPEN: ${t2 - t0}ms (connect=${t2 - t1}ms)")

            if (!session.sendText(buildSetupPayload(request))) {
                sink.offer(ProviderAudioEvent.Error("Gemini TTS setup payload was rejected."))
                return
            }
            val t3 = SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  Setup payload sent: ${t3 - t0}ms")

            var setupComplete = false
            val deadline = SystemClock.elapsedRealtime() + 10_000
            while (!setupComplete && !isStale()) {
                val event = session.poll(50)
                when (event) {
                    null -> {
                        if (SystemClock.elapsedRealtime() >= deadline) {
                            sink.offer(ProviderAudioEvent.Error("Gemini TTS setup timed out."))
                            return
                        }
                    }

                    is WebSocketEvent.Text -> {
                        if (event.payload.contains("setupComplete")) {
                            setupComplete = true
                            android.util.Log.d("TTS-Timing", "  Setup COMPLETE: ${SystemClock.elapsedRealtime() - t0}ms")
                        } else {
                            parseError(event.payload)?.let {
                                sink.offer(ProviderAudioEvent.Error(it))
                                return
                            }
                        }
                    }

                    is WebSocketEvent.Binary -> {
                        val text = event.payload.utf8()
                        if (text.contains("setupComplete")) {
                            setupComplete = true
                            android.util.Log.d("TTS-Timing", "  Setup COMPLETE (binary): ${SystemClock.elapsedRealtime() - t0}ms")
                        } else {
                            parseError(text)?.let {
                                sink.offer(ProviderAudioEvent.Error(it))
                                return
                            }
                        }
                    }

                    is WebSocketEvent.Failure -> {
                        sink.offer(ProviderAudioEvent.Error(event.throwable.message ?: "Gemini TTS websocket failed."))
                        return
                    }

                    WebSocketEvent.Closed -> {
                        sink.offer(ProviderAudioEvent.Error("Gemini TTS websocket closed before setup completed."))
                        return
                    }
                }
            }

            if (isStale()) {
                return
            }

            if (!session.sendText(buildTextPayload(request.text))) {
                sink.offer(ProviderAudioEvent.Error("Gemini TTS text payload was rejected."))
                return
            }
            val t4 = SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  Text payload sent: ${t4 - t0}ms")

            var firstAudioLogged = false
            while (!isStale()) {
                when (val event = session.poll(250)) {
                    null -> Unit
                    is WebSocketEvent.Text -> {
                        parseError(event.payload)?.let {
                            sink.offer(ProviderAudioEvent.Error(it))
                            return
                        }
                        parsePcmInlineData(event.payload)?.let {
                            if (!firstAudioLogged) {
                                firstAudioLogged = true
                                android.util.Log.d("TTS-Timing", "  FIRST AUDIO chunk: ${SystemClock.elapsedRealtime() - t0}ms (since text=${SystemClock.elapsedRealtime() - t4}ms)")
                            }
                            sink.offer(ProviderAudioEvent.PcmData(it))
                        }
                        if (isTurnComplete(event.payload)) {
                            sink.offer(ProviderAudioEvent.End)
                            return
                        }
                    }

                    is WebSocketEvent.Binary -> {
                        val text = event.payload.utf8()
                        parseError(text)?.let {
                            sink.offer(ProviderAudioEvent.Error(it))
                            return
                        }
                        parsePcmInlineData(text)?.let {
                            if (!firstAudioLogged) {
                                firstAudioLogged = true
                                android.util.Log.d("TTS-Timing", "  FIRST AUDIO chunk (binary): ${SystemClock.elapsedRealtime() - t0}ms (since text=${SystemClock.elapsedRealtime() - t4}ms)")
                            }
                            sink.offer(ProviderAudioEvent.PcmData(it))
                        }
                        if (isTurnComplete(text)) {
                            android.util.Log.d("TTS-Timing", "  TURN COMPLETE: ${SystemClock.elapsedRealtime() - t0}ms")
                            sink.offer(ProviderAudioEvent.End)
                            return
                        }
                    }

                    is WebSocketEvent.Failure -> {
                        sink.offer(ProviderAudioEvent.Error(event.throwable.message ?: "Gemini TTS websocket failed."))
                        return
                    }

                    WebSocketEvent.Closed -> {
                        sink.offer(ProviderAudioEvent.End)
                        return
                    }
                }
            }
        }
        // Warm-up is handled by the worker loop after stream() returns
    }

    /** Stream audio from an already-connected session. Setup remains request-specific. */
    private fun streamWithSession(
        session: BlockingWebSocketSession,
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
        t0: Long,
    ) {
        session.use {
            if (!session.sendText(buildSetupPayload(request))) {
                sink.offer(ProviderAudioEvent.Error("Gemini TTS setup payload was rejected."))
                return
            }
            val t3 = SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  Setup payload sent (warm): ${t3 - t0}ms")

            var setupComplete = false
            val deadline = SystemClock.elapsedRealtime() + 10_000
            while (!setupComplete && !isStale()) {
                when (val event = session.poll(50)) {
                    null -> {
                        if (SystemClock.elapsedRealtime() >= deadline) {
                            sink.offer(ProviderAudioEvent.Error("Gemini TTS setup timed out."))
                            return
                        }
                    }

                    is WebSocketEvent.Text -> {
                        if (event.payload.contains("setupComplete")) {
                            setupComplete = true
                            android.util.Log.d("TTS-Timing", "  Setup COMPLETE (warm): ${SystemClock.elapsedRealtime() - t0}ms")
                        } else {
                            parseError(event.payload)?.let {
                                sink.offer(ProviderAudioEvent.Error(it))
                                return
                            }
                        }
                    }

                    is WebSocketEvent.Binary -> {
                        val text = event.payload.utf8()
                        if (text.contains("setupComplete")) {
                            setupComplete = true
                            android.util.Log.d("TTS-Timing", "  Setup COMPLETE (warm binary): ${SystemClock.elapsedRealtime() - t0}ms")
                        } else {
                            parseError(text)?.let {
                                sink.offer(ProviderAudioEvent.Error(it))
                                return
                            }
                        }
                    }

                    is WebSocketEvent.Failure -> {
                        sink.offer(ProviderAudioEvent.Error(event.throwable.message ?: "Gemini TTS websocket failed."))
                        return
                    }

                    WebSocketEvent.Closed -> {
                        sink.offer(ProviderAudioEvent.Error("Gemini TTS websocket closed before setup completed."))
                        return
                    }
                }
            }

            if (isStale()) {
                return
            }

            if (!session.sendText(buildTextPayload(request.text))) {
                sink.offer(ProviderAudioEvent.Error("Gemini TTS text payload was rejected."))
                return
            }
            val t4 = SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  Text payload sent (warm): ${t4 - t0}ms")

            var firstAudioLogged = false
            while (!isStale()) {
                when (val event = session.poll(250)) {
                    null -> Unit
                    is WebSocketEvent.Text -> {
                        parseError(event.payload)?.let {
                            sink.offer(ProviderAudioEvent.Error(it))
                            return
                        }
                        parsePcmInlineData(event.payload)?.let {
                            if (!firstAudioLogged) {
                                firstAudioLogged = true
                                android.util.Log.d("TTS-Timing", "  FIRST AUDIO (warm): ${SystemClock.elapsedRealtime() - t0}ms")
                            }
                            sink.offer(ProviderAudioEvent.PcmData(it))
                        }
                        if (isTurnComplete(event.payload)) {
                            android.util.Log.d("TTS-Timing", "  TURN COMPLETE (warm): ${SystemClock.elapsedRealtime() - t0}ms")
                            sink.offer(ProviderAudioEvent.End)
                            return
                        }
                    }
                    is WebSocketEvent.Binary -> {
                        val text = event.payload.utf8()
                        parseError(text)?.let {
                            sink.offer(ProviderAudioEvent.Error(it))
                            return
                        }
                        parsePcmInlineData(text)?.let {
                            if (!firstAudioLogged) {
                                firstAudioLogged = true
                                android.util.Log.d("TTS-Timing", "  FIRST AUDIO (warm binary): ${SystemClock.elapsedRealtime() - t0}ms")
                            }
                            sink.offer(ProviderAudioEvent.PcmData(it))
                        }
                        if (isTurnComplete(text)) {
                            android.util.Log.d("TTS-Timing", "  TURN COMPLETE (warm): ${SystemClock.elapsedRealtime() - t0}ms")
                            sink.offer(ProviderAudioEvent.End)
                            return
                        }
                    }
                    is WebSocketEvent.Failure -> {
                        sink.offer(ProviderAudioEvent.Error(event.throwable.message ?: "Gemini TTS websocket failed."))
                        return
                    }
                    WebSocketEvent.Closed -> {
                        sink.offer(ProviderAudioEvent.End)
                        return
                    }
                }
            }
        }
    }

    private fun buildSetupPayload(request: TtsRequest): String {
        val settings = request.settingsSnapshot
        val speedLabel = if (request.requestMode == TtsRequestMode.REALTIME) {
            "Normal"
        } else {
            settings.speedPreset.toGeminiSpeedLabel()
        }

        var systemText = "You are a text-to-speech reader. Your ONLY job is to read the user's text out loud, exactly as written, word for word. Do NOT respond conversationally. Do NOT add commentary. Do NOT ask questions. "
        systemText += when (speedLabel) {
            "Slow" -> "Speak slowly, clearly, and with deliberate pacing. "
            "Fast" -> "Speak quickly, efficiently, and with a brisk pace. "
            else -> "Simply read the provided text aloud naturally and clearly. "
        }
        languageInstruction(settings.languageConditions, request.text)?.takeIf(String::isNotBlank)?.let {
            systemText += " Additional instructions: ${it.trim()} "
        }
        systemText += "Start reading immediately."

        return JSONObject()
            .put(
                "setup",
                JSONObject()
                    .put("model", "models/$GEMINI_TTS_MODEL")
                    .put(
                        "generationConfig",
                        JSONObject()
                            .put("responseModalities", JSONArray().put("AUDIO"))
                            .put(
                                "speechConfig",
                                JSONObject().put(
                                    "voiceConfig",
                                    JSONObject().put(
                                        "prebuiltVoiceConfig",
                                        JSONObject().put("voiceName", settings.geminiVoice),
                                    ),
                                ),
                            )
                            .put(
                                "thinkingConfig",
                                JSONObject().put("thinkingBudget", 0),
                            ),
                    )
                    .put(
                        "systemInstruction",
                        JSONObject().put(
                            "parts",
                            JSONArray().put(JSONObject().put("text", systemText)),
                        ),
                    ),
            )
            .toString()
    }

    private fun buildTextPayload(text: String): String {
        val prompt = "[READ ALOUD VERBATIM - START NOW]\n\n$text"
        return JSONObject()
            .put(
                "clientContent",
                JSONObject()
                    .put(
                        "turns",
                        JSONArray().put(
                            JSONObject()
                                .put("role", "user")
                                .put(
                                    "parts",
                                    JSONArray().put(JSONObject().put("text", prompt)),
                                ),
                        ),
                    )
                    .put("turnComplete", true),
            )
            .toString()
    }

    private fun parsePcmInlineData(message: String): ByteArray? {
        return runCatching {
            val parts = JSONObject(message)
                .optJSONObject("serverContent")
                ?.optJSONObject("modelTurn")
                ?.optJSONArray("parts")
                ?: return@runCatching null
            for (index in 0 until parts.length()) {
                val base64 = parts.optJSONObject(index)
                    ?.optJSONObject("inlineData")
                    ?.optString("data")
                    ?.takeIf(String::isNotBlank)
                    ?: continue
                return@runCatching Base64.decode(base64, Base64.DEFAULT)
            }
            null
        }.getOrNull()
    }

    private fun isTurnComplete(message: String): Boolean {
        return runCatching {
            val serverContent = JSONObject(message).optJSONObject("serverContent") ?: return@runCatching false
            serverContent.optBoolean("turnComplete") || serverContent.optBoolean("generationComplete")
        }.getOrDefault(false)
    }

    private fun parseError(message: String): String? {
        return runCatching {
            JSONObject(message).optJSONObject("error")?.optString("message")?.takeIf(String::isNotBlank)
        }.getOrNull()
    }

    private fun languageInstruction(
        conditions: List<dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition>,
        text: String,
    ): String? {
        val detectedCode = languageDetector.detectIso639_3(text)
        return conditions.firstOrNull {
            it.languageCode.equals(detectedCode, ignoreCase = true)
        }?.instruction
    }

    private fun MobileTtsSpeedPreset.toGeminiSpeedLabel(): String {
        return when (this) {
            MobileTtsSpeedPreset.SLOW -> "Slow"
            MobileTtsSpeedPreset.FAST -> "Fast"
            MobileTtsSpeedPreset.NORMAL -> "Normal"
        }
    }

    private companion object {
        private const val GEMINI_TTS_MODEL = "gemini-2.5-flash-native-audio-preview-12-2025"
        private const val LIVE_WS_ENDPOINT =
            "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
    }
}

internal class EdgeTtsProvider(
    private val httpClient: OkHttpClient,
    private val languageDetector: DeviceLanguageDetector,
    private val mp3Decoder: Mp3Decoder,
) {
    fun stream(
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        val voiceName = resolveVoice(request)
        val settings = request.settingsSnapshot.edgeSettings
        val connectionId = UUID.randomUUID().toString().replace("-", "")

        val secMsGec = generateSecMsGec()
        val socketRequest = Request.Builder()
            .url("$EDGE_WS_ENDPOINT?TrustedClientToken=$EDGE_TRUSTED_TOKEN&ConnectionId=$connectionId&Sec-MS-GEC=$secMsGec&Sec-MS-GEC-Version=$SEC_MS_GEC_VERSION")
            .header("Origin", EDGE_ORIGIN)
            .header("Pragma", "no-cache")
            .header("Cache-Control", "no-cache")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/$CHROMIUM_FULL_VERSION Safari/537.36 Edg/$CHROMIUM_FULL_VERSION")
            .build()

        BlockingWebSocketSession(httpClient, socketRequest).use { session ->
            if (!session.awaitOpen(10_000)) {
                val failEvent = session.poll(0)
                val reason = if (failEvent is WebSocketEvent.Failure) failEvent.throwable.message else "timeout"
                sink.offer(ProviderAudioEvent.Error("Edge TTS websocket failed: $reason"))
                return
            }

            val configMessage =
                "X-Timestamp:${edgeHeaderTimestamp()}\r\n" +
                    "Content-Type:application/json; charset=utf-8\r\n" +
                    "Path:speech.config\r\n\r\n" +
                    "{\"context\":{\"synthesis\":{\"audio\":{\"metadataoptions\":{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}"

            if (!session.sendText(configMessage)) {

                sink.offer(ProviderAudioEvent.Error("Edge TTS config payload was rejected."))
                return
            }

            val ssml = buildEdgeSsml(
                text = request.text,
                voiceName = voiceName,
                pitch = settings.pitch,
                rate = settings.rate,
                volume = settings.volume,
            )
            val ssmlMessage =
                "X-RequestId:$connectionId\r\n" +
                    "Content-Type:application/ssml+xml\r\n" +
                    "X-Timestamp:${edgeSsmlTimestamp()}Z\r\n" +
                    "Path:ssml\r\n\r\n$ssml"
            if (!session.sendText(ssmlMessage)) {

                sink.offer(ProviderAudioEvent.Error("Edge TTS SSML payload was rejected."))
                return
            }

            val mp3Data = ArrayList<Byte>()
            while (!isStale()) {
                when (val event = session.poll(250)) {
                    null -> Unit
                    is WebSocketEvent.Binary -> {
                        val bytes = event.payload.toByteArray()
                        if (bytes.size >= 2) {
                            val headerLength = ((bytes[0].toInt() and 0xFF) shl 8) or (bytes[1].toInt() and 0xFF)
                            val audioStart = 2 + headerLength
                            if (bytes.size > audioStart) {
                                val header = bytes.copyOfRange(2, audioStart)
                                if (header.toString(StandardCharsets.UTF_8).contains("Path:audio")) {
                                    bytes.copyOfRange(audioStart, bytes.size).forEach(mp3Data::add)
                                }
                            }
                        }
                    }

                    is WebSocketEvent.Text -> {

                        if (event.payload.contains("Path:turn.end")) {

                            emitDecodedMp3(mp3Data.toByteArray(), sink)
                            return
                        }
                    }

                    is WebSocketEvent.Failure -> {

                        sink.offer(ProviderAudioEvent.Error(event.throwable.message ?: "Edge TTS websocket failed."))
                        return
                    }

                    WebSocketEvent.Closed -> {

                        emitDecodedMp3(mp3Data.toByteArray(), sink)
                        return
                    }
                }
            }

        }
    }

    private fun resolveVoice(request: TtsRequest): String {
        val detectedLanguage = languageDetector.detectIso639_1(request.text)
        val configs = request.settingsSnapshot.edgeSettings.voiceConfigs
        val matched = configs.firstOrNull {
            it.languageCode.equals(detectedLanguage, ignoreCase = true)
        }
        val voice = matched?.voiceName ?: DEFAULT_EDGE_VOICE

        return voice
    }

    private fun emitDecodedMp3(
        mp3: ByteArray,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {

        val pcm = try {
            mp3Decoder.decodeToMonoPcm24k(mp3)
        } catch (e: Exception) {

            sink.offer(ProviderAudioEvent.Error("Edge TTS MP3 decode failed: ${e.message}"))
            return
        }

        if (pcm.isEmpty()) {
            sink.offer(ProviderAudioEvent.Error("Edge TTS did not return playable audio."))
            return
        }
        chunkBytes(pcm, CHUNK_BYTES).forEach { chunk ->
            sink.offer(ProviderAudioEvent.PcmData(chunk))
        }
        sink.offer(ProviderAudioEvent.End)
    }

    private fun buildEdgeSsml(
        text: String,
        voiceName: String,
        pitch: Int,
        rate: Int,
        volume: Int,
    ): String {
        val pitchString = if (pitch >= 0) "+${pitch}Hz" else "${pitch}Hz"
        val rateString = if (rate >= 0) "+${rate}%" else "${rate}%"
        val volumeString = if (volume >= 0) "+${volume}%" else "${volume}%"
        val escaped = text
            .replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
            .replace("\"", "&quot;")
            .replace("'", "&apos;")

        return "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>" +
            "<voice name='$voiceName'>" +
            "<prosody pitch='$pitchString' rate='$rateString' volume='$volumeString'>$escaped</prosody>" +
            "</voice></speak>"
    }

    private fun edgeHeaderTimestamp(): String {
        return java.time.ZonedDateTime.now(java.time.ZoneOffset.UTC)
            .format(java.time.format.DateTimeFormatter.ofPattern("EEE MMM dd yyyy HH:mm:ss 'GMT+0000 (Coordinated Universal Time)'", java.util.Locale.US))
    }

    private fun edgeSsmlTimestamp(): String {
        return java.time.ZonedDateTime.now(java.time.ZoneOffset.UTC)
            .format(java.time.format.DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss.SSS", java.util.Locale.US))
    }

    private fun generateSecMsGec(): String {
        val winEpochOffset = 11644473600L
        val nowSeconds = System.currentTimeMillis() / 1000
        val adjustedSeconds = nowSeconds + winEpochOffset
        val roundedSeconds = adjustedSeconds - (adjustedSeconds % 300)
        val ticks = roundedSeconds * 10_000_000L
        val input = "$ticks$EDGE_TRUSTED_TOKEN"
        val digest = java.security.MessageDigest.getInstance("SHA-256").digest(input.toByteArray())
        return digest.joinToString("") { "%02X".format(it) }
    }

    private companion object {
        private const val EDGE_TRUSTED_TOKEN = "6A5AA1D4EAFF4E9FB37E23D68491D6F4"
        private const val EDGE_WS_ENDPOINT =
            "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1"
        private const val EDGE_ORIGIN = "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold"
        private const val CHROMIUM_FULL_VERSION = "143.0.3650.75"
        private const val SEC_MS_GEC_VERSION = "1-$CHROMIUM_FULL_VERSION"
        private const val DEFAULT_EDGE_VOICE = "en-US-AriaNeural"
        private const val CHUNK_BYTES = 24_000
    }
}

internal class GoogleTranslateTtsProvider(
    private val httpClient: OkHttpClient,
    private val languageDetector: DeviceLanguageDetector,
    private val mp3Decoder: Mp3Decoder,
) {
    fun stream(
        request: TtsRequest,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        val languageCode = languageDetector.detectIso639_1(request.text)
        val encodedText = URLEncoder.encode(request.text, StandardCharsets.UTF_8.name())
        val url =
            "https://translate.google.com/translate_tts?ie=UTF-8&q=$encodedText&tl=$languageCode&client=tw-ob"

        val httpRequest = Request.Builder()
            .url(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .build()

        runCatching {
            httpClient.newCall(httpRequest).execute().use { response ->
                check(response.isSuccessful) { "Google TTS HTTP ${response.code}" }
                val mp3Bytes = response.body?.bytes() ?: ByteArray(0)
                val pcm = mp3Decoder.decodeToMonoPcm24k(mp3Bytes)
                check(pcm.isNotEmpty()) { "Google TTS audio decode failed." }
                chunkBytes(pcm, CHUNK_BYTES).forEach { chunk ->
                    sink.offer(ProviderAudioEvent.PcmData(chunk))
                }
                sink.offer(ProviderAudioEvent.End)
            }
        }.onFailure { error ->
            sink.offer(ProviderAudioEvent.Error(error.message ?: "Google Translate TTS failed."))
        }
    }

    private companion object {
        private const val CHUNK_BYTES = 24_000
    }
}

private fun chunkBytes(
    bytes: ByteArray,
    chunkSize: Int,
): Sequence<ByteArray> {
    return sequence {
        var index = 0
        while (index < bytes.size) {
            val end = (index + chunkSize).coerceAtMost(bytes.size)
            yield(bytes.copyOfRange(index, end))
            index = end
        }
    }
}
