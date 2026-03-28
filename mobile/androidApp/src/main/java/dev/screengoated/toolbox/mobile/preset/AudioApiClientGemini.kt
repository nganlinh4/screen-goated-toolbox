package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.ensureActive
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.coroutineContext

internal suspend fun AudioApiClient.transcribeWithGemini(
    model: PresetModelDescriptor,
    prompt: String,
    wavBytes: ByteArray,
    apiKey: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")
    val payload = buildGeminiAudioPayload(
        model = model,
        prompt = prompt,
        wavBytes = wavBytes,
    )
    val action = if (streamingEnabled) "streamGenerateContent?alt=sse" else "generateContent"
    val request = Request.Builder()
        .url("$GEMINI_ENDPOINT/${model.fullName}:$action")
        .header("x-goog-api-key", apiKey)
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    return if (streamingEnabled) {
        val fullContent = StringBuilder()
        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("Gemini audio request failed with $code")
            }
            val body = response.body ?: throw IOException("Gemini audio response body was empty.")
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    coroutineContext.ensureActive()
                    val line = rawLine.trim()
                    if (!line.startsWith("data: ")) return@forEach
                    val data = line.removePrefix("data: ").trim()
                    if (data.isBlank() || data == "[DONE]") return@forEach
                    val chunk = extractGeminiAudioDelta(data)
                    if (chunk.isNotEmpty()) {
                        fullContent.append(chunk)
                        onChunk(chunk)
                    }
                }
            }
        }
        fullContent.toString()
    } else {
        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("Gemini audio request failed with $code")
            }
            val body = response.body?.string().orEmpty()
            extractGeminiAudioDelta(body)
        }
    }
}

internal suspend fun AudioApiClient.transcribeWithGeminiLiveInput(
    model: PresetModelDescriptor,
    wavBytes: ByteArray,
    apiKey: String,
    onChunk: (String) -> Unit,
): String {
    val session = openGeminiLiveInputSession(
        model = model,
        apiKey = apiKey,
        onChunk = onChunk,
    )
    val samples = PresetAudioCodec.decodePcm16MonoWav(wavBytes)
    try {
        val chunkSize = 1_600
        var offset = 0
        while (offset < samples.size) {
            coroutineContext.ensureActive()
            val end = (offset + chunkSize).coerceAtMost(samples.size)
            session.appendPcm16Chunk(samples.copyOfRange(offset, end))
            offset = end
            kotlinx.coroutines.delay(10)
        }
        return session.finish().transcript
    } finally {
        session.cancel()
    }
}

internal suspend fun AudioApiClient.openGeminiLiveInputSession(
    model: PresetModelDescriptor,
    apiKey: String,
    onChunk: (String) -> Unit,
): AudioStreamingSession {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")
    val events = LinkedBlockingDeque<GeminiLiveInputEvent>()
    val setupReady = kotlinx.coroutines.CompletableDeferred<Unit>()
    val transcript = StringBuilder()
    val finalTranscript = StringBuilder()
    val closed = AtomicBoolean(false)
    val socket = httpClient.newWebSocket(
        Request.Builder().url("wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key=$apiKey").build(),
        object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                webSocket.send(
                    JSONObject()
                        .put(
                            "setup",
                            JSONObject().put(
                                "model",
                                "models/${model.fullName}",
                            ).put(
                                "generationConfig",
                                JSONObject()
                                    .put("responseModalities", JSONArray().put("AUDIO"))
                                    .put("mediaResolution", "MEDIA_RESOLUTION_LOW")
                                    .put("thinkingConfig", JSONObject().put("thinkingBudget", 0)),
                            ).put(
                                "inputAudioTranscription",
                                JSONObject(),
                            ),
                        )
                        .apply {
                            if (model.fullName == "gemini-3.1-flash-live-preview") {
                                getJSONObject("setup").put(
                                    "realtimeInputConfig",
                                    JSONObject()
                                        .put(
                                            "automaticActivityDetection",
                                            JSONObject()
                                                .put("startOfSpeechSensitivity", "START_SENSITIVITY_HIGH")
                                                .put("endOfSpeechSensitivity", "END_SENSITIVITY_HIGH")
                                                .put("prefixPaddingMs", 80)
                                                .put("silenceDurationMs", 320),
                                        )
                                        .put("turnCoverage", "TURN_INCLUDES_ONLY_ACTIVITY"),
                                )
                            }
                        }
                        .toString(),
                )
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                handleGeminiLiveMessage(
                    message = text,
                    setupReady = setupReady,
                    events = events,
                    transcript = transcript,
                    finalTranscript = finalTranscript,
                    onChunk = onChunk,
                )
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                handleGeminiLiveMessage(
                    message = bytes.utf8(),
                    setupReady = setupReady,
                    events = events,
                    transcript = transcript,
                    finalTranscript = finalTranscript,
                    onChunk = onChunk,
                )
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (!setupReady.isCompleted) {
                    setupReady.completeExceptionally(t)
                }
                events.offer(GeminiLiveInputEvent.Error(t.message ?: "Gemini Live websocket failed."))
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                events.offer(GeminiLiveInputEvent.Closed)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                events.offer(GeminiLiveInputEvent.Closed)
            }
        },
    )
    kotlinx.coroutines.withTimeout(20_000) { setupReady.await() }
    return object : AudioStreamingSession {
        override suspend fun appendPcm16Chunk(chunk: ShortArray) {
            coroutineContext.ensureActive()
            val payload = JSONObject()
                .put(
                    "realtimeInput",
                    JSONObject().put(
                        "audio",
                        JSONObject()
                            .put("mimeType", "audio/pcm;rate=16000")
                            .put(
                                "data",
                                android.util.Base64.encodeToString(shortArrayToLittleEndianBytes(chunk), android.util.Base64.NO_WRAP),
                            ),
                    ),
                )
            if (!socket.send(payload.toString())) {
                throw IOException("Gemini Live audio chunk was rejected.")
            }
        }

        override suspend fun finish(): AudioStreamingTranscriptResult {
            socket.send(JSONObject().put("realtimeInput", JSONObject().put("audioStreamEnd", true)).toString())
            val concludeUntil = System.currentTimeMillis() + 2_000
            while (System.currentTimeMillis() < concludeUntil) {
                coroutineContext.ensureActive()
                when (val event = events.poll()) {
                    is GeminiLiveInputEvent.Error -> throw IOException(event.message)
                    GeminiLiveInputEvent.Closed -> break
                    null -> kotlinx.coroutines.delay(50)
                }
            }
            closeSocketIfNeeded(socket, closed)
            return AudioStreamingTranscriptResult(
                transcript = finalTranscript.toString(),
                producedRealtimePaste = false,
            )
        }

        override fun cancel() {
            closeSocketIfNeeded(socket, closed)
        }
    }
}

private fun buildGeminiAudioPayload(
    model: PresetModelDescriptor,
    prompt: String,
    wavBytes: ByteArray,
): JSONObject {
    val contentParts = JSONArray()
    if (prompt.isNotBlank()) {
        contentParts.put(JSONObject().put("text", prompt))
    }
    contentParts.put(
        JSONObject().put(
            "inline_data",
            JSONObject()
                .put("mime_type", "audio/wav")
                .put(
                    "data",
                    android.util.Base64.encodeToString(wavBytes, android.util.Base64.NO_WRAP),
                ),
        ),
    )
    return JSONObject().put(
        "contents",
        JSONArray().put(
            JSONObject()
                .put("role", "user")
                .put("parts", contentParts),
        ),
    )
}

private fun extractGeminiAudioDelta(payload: String): String {
    val root = JSONObject(payload)
    val candidates = root.optJSONArray("candidates") ?: return ""
    val candidate = candidates.optJSONObject(0) ?: return ""
    val parts = candidate.optJSONObject("content")?.optJSONArray("parts") ?: return ""
    val result = StringBuilder()
    for (index in 0 until parts.length()) {
        val part = parts.optJSONObject(index) ?: continue
        if (part.optBoolean("thought", false)) continue
        result.append(part.optString("text"))
    }
    return result.toString()
}

private sealed interface GeminiLiveInputEvent {
    data class Error(val message: String) : GeminiLiveInputEvent
    data object Closed : GeminiLiveInputEvent
}

private fun handleGeminiLiveMessage(
    message: String,
    setupReady: kotlinx.coroutines.CompletableDeferred<Unit>,
    events: LinkedBlockingDeque<GeminiLiveInputEvent>,
    transcript: StringBuilder,
    finalTranscript: StringBuilder,
    onChunk: (String) -> Unit,
) {
    if (message.contains("setupComplete")) {
        if (!setupReady.isCompleted) {
            setupReady.complete(Unit)
        }
        return
    }
    if (message.contains("\"error\"")) {
        val root = runCatching { JSONObject(message) }.getOrNull()
        val error = root?.optJSONObject("error")?.optString("message")
            ?: root?.optString("error")
            ?: "Gemini Live audio transcription failed."
        events.offer(GeminiLiveInputEvent.Error(error))
        return
    }
    val text = extractGeminiLiveInputTranscript(message)
    if (text.isNotBlank()) {
        val delta = transcriptDelta(transcript.toString(), text)
        if (delta.isNotEmpty()) {
            transcript.append(delta)
            finalTranscript.clear()
            finalTranscript.append(transcript)
            onChunk(delta)
        }
    }
}

private fun extractGeminiLiveInputTranscript(message: String): String {
    val root = runCatching { JSONObject(message) }.getOrNull() ?: return ""
    val serverContent = root.optJSONObject("serverContent") ?: return ""
    return serverContent.optJSONObject("inputTranscription")?.optString("text").orEmpty()
}

private fun shortArrayToLittleEndianBytes(samples: ShortArray): ByteArray {
    val bytes = ByteArray(samples.size * 2)
    var offset = 0
    samples.forEach { sample ->
        bytes[offset++] = (sample.toInt() and 0xFF).toByte()
        bytes[offset++] = ((sample.toInt() shr 8) and 0xFF).toByte()
    }
    return bytes
}

private fun transcriptDelta(current: String, next: String): String {
    if (next.startsWith(current)) {
        return next.removePrefix(current)
    }
    return next
}

private fun closeSocketIfNeeded(socket: WebSocket, closed: AtomicBoolean) {
    if (closed.compareAndSet(false, true)) {
        socket.close(1000, "done")
        socket.cancel()
    }
}
