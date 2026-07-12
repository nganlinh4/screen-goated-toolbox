package dev.screengoated.toolbox.mobile.service.tts

import android.os.SystemClock
import android.util.Base64
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.service.LIVE_WS_ENDPOINT
import dev.screengoated.toolbox.mobile.shared.live.geminiLiveThinkingJson
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONObject
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
            sink.offer(ProviderAudioEvent.Error("NO_API_KEY:google"))
            return
        }

        val t0 = SystemClock.elapsedRealtime()
        android.util.Log.d("TTS-Timing", "▶ START request: '${request.text.take(30)}...'")

        // Try warm socket first (skips connect time only; setup stays request-specific)
        val warm = acquireWarmSession(apiKey)
        android.util.Log.d("TTS-Timing", "  Warm socket check: ${if (warm != null) "AVAILABLE" else "not available"}")
        if (warm != null) {
            android.util.Log.d("TTS-Timing", "  Using WARM socket (connect already done)")
            streamFromOpenSession(warm, request, isStale, sink, t0, warmLabel = " (warm)")
            // warmUp is called by the worker loop after stream() returns
            return
        }

        val socketRequest = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()

        val session = BlockingWebSocketSession(httpClient, socketRequest)
        val t1 = SystemClock.elapsedRealtime()
        android.util.Log.d("TTS-Timing", "  WebSocket object created: ${t1 - t0}ms")

        if (!session.awaitOpen(10_000)) {
            session.close()
            sink.offer(ProviderAudioEvent.Error("Gemini TTS websocket failed to open."))
            return
        }
        val t2 = SystemClock.elapsedRealtime()
        android.util.Log.d("TTS-Timing", "  WebSocket OPEN: ${t2 - t0}ms (connect=${t2 - t1}ms)")

        streamFromOpenSession(session, request, isStale, sink, t0, warmLabel = "")
        // Warm-up is handled by the worker loop after stream() returns
    }

    /** Stream audio from an already-connected session. Setup remains request-specific. */
    private fun streamFromOpenSession(
        session: BlockingWebSocketSession,
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
        t0: Long,
        warmLabel: String,
    ) {
        session.use {
            if (!session.sendText(buildSetupPayload(request))) {
                sink.offer(ProviderAudioEvent.Error("Gemini TTS setup payload was rejected."))
                return
            }
            val t3 = SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  Setup payload sent$warmLabel: ${t3 - t0}ms")

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
                            android.util.Log.d("TTS-Timing", "  Setup COMPLETE$warmLabel: ${SystemClock.elapsedRealtime() - t0}ms")
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
                            android.util.Log.d("TTS-Timing", "  Setup COMPLETE$warmLabel (binary): ${SystemClock.elapsedRealtime() - t0}ms")
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
            android.util.Log.d("TTS-Timing", "  Text payload sent$warmLabel: ${t4 - t0}ms")

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
                                android.util.Log.d("TTS-Timing", "  FIRST AUDIO chunk$warmLabel: ${SystemClock.elapsedRealtime() - t0}ms (since text=${SystemClock.elapsedRealtime() - t4}ms)")
                            }
                            sink.offer(ProviderAudioEvent.PcmData(it))
                        }
                        if (isTurnComplete(event.payload)) {
                            android.util.Log.d("TTS-Timing", "  TURN COMPLETE$warmLabel: ${SystemClock.elapsedRealtime() - t0}ms")
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
                                android.util.Log.d("TTS-Timing", "  FIRST AUDIO chunk$warmLabel (binary): ${SystemClock.elapsedRealtime() - t0}ms (since text=${SystemClock.elapsedRealtime() - t4}ms)")
                            }
                            sink.offer(ProviderAudioEvent.PcmData(it))
                        }
                        if (isTurnComplete(text)) {
                            android.util.Log.d("TTS-Timing", "  TURN COMPLETE$warmLabel: ${SystemClock.elapsedRealtime() - t0}ms")
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

        val generationConfig = JSONObject()
            .put("responseModalities", JSONArray().put("AUDIO"))
            .put("mediaResolution", "MEDIA_RESOLUTION_LOW")
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

        geminiLiveThinkingJson(settings.geminiModel)?.let { generationConfig.put("thinkingConfig", it) }

        return JSONObject()
            .put(
                "setup",
                JSONObject()
                    .put("model", "models/${settings.geminiModel}")
                    .put("generationConfig", generationConfig)
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
            .put("realtimeInput", JSONObject().put("text", prompt))
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
}

internal fun chunkBytes(
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
