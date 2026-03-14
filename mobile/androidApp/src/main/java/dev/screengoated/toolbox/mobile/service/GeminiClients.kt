package dev.screengoated.toolbox.mobile.service

import android.util.Base64
import android.util.Log
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.shared.live.TranslationRequest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.json.JSONException
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlin.random.Random

class GeminiLiveSocketClient(
    private val httpClient: OkHttpClient,
) {
    private enum class AudioMode { NORMAL, SILENCE, CATCH_UP }

    /**
     * Runs a long-lived Gemini Live session with automatic WebSocket reconnection,
     * matching the Windows audio streaming implementation:
     * - Normal mode: stream real audio for 20s
     * - Silence mode: send silence for 2s while buffering real audio
     * - CatchUp mode: replay buffered audio at 2x speed
     * - On connection loss/stall: reconnect and enter CatchUp
     */
    suspend fun runSession(
        apiKey: String,
        audioChunks: Flow<ShortArray>,
        onTranscript: (String) -> Unit,
    ) {
        val audioBuffer = LinkedBlockingDeque<ShortArray>()
        var silenceBuffer = mutableListOf<Short>()
        var audioMode = AudioMode.NORMAL
        var modeStartMs = System.currentTimeMillis()
        var lastTranscriptionMs = System.currentTimeMillis()
        var consecutiveEmptyPolls = 0
        var outboundChunks = 0

        // Connect initial socket
        var session = connectAndSetup(apiKey, onTranscript)
            ?: throw IOException("Gemini Live initial connection failed.")

        coroutineScope {
        // Collect audio in background, buffer it for the streaming loop
        val collectJob = launch(Dispatchers.IO) {
            audioChunks.collect { chunk ->
                audioBuffer.offer(chunk)
            }
        }

        try {
            while (isActive && !collectJob.isCancelled) {
                // Audio mode state machine transitions
                val elapsed = System.currentTimeMillis() - modeStartMs
                when (audioMode) {
                    AudioMode.NORMAL -> {
                        if (elapsed >= NORMAL_DURATION_MS) {
                            audioMode = AudioMode.SILENCE
                            modeStartMs = System.currentTimeMillis()
                            silenceBuffer.clear()
                        }
                    }
                    AudioMode.SILENCE -> {
                        if (elapsed >= SILENCE_DURATION_MS) {
                            audioMode = AudioMode.CATCH_UP
                            modeStartMs = System.currentTimeMillis()
                        }
                    }
                    AudioMode.CATCH_UP -> {
                        if (silenceBuffer.isEmpty()) {
                            audioMode = AudioMode.NORMAL
                            modeStartMs = System.currentTimeMillis()
                        }
                    }
                }

                // Drain audio buffer
                val realAudio = mutableListOf<Short>()
                while (true) {
                    val chunk = audioBuffer.poll() ?: break
                    for (s in chunk) realAudio.add(s)
                }

                // Send audio based on mode
                val sendOk = when (audioMode) {
                    AudioMode.NORMAL -> {
                        if (realAudio.isNotEmpty()) {
                            sendChunked(session.socket, realAudio.toShortArray(), CHUNK_SIZE).also {
                                outboundChunks++
                            }
                        } else {
                            true
                        }
                    }
                    AudioMode.SILENCE -> {
                        silenceBuffer.addAll(realAudio)
                        val silence = ShortArray(SAMPLES_PER_100MS)
                        sendChunked(session.socket, silence, CHUNK_SIZE)
                    }
                    AudioMode.CATCH_UP -> {
                        silenceBuffer.addAll(realAudio)
                        val doubleChunk = SAMPLES_PER_100MS * 2
                        if (silenceBuffer.size >= doubleChunk) {
                            val toSend = ShortArray(doubleChunk) { silenceBuffer.removeFirst() }
                            sendChunked(session.socket, toSend, CHUNK_SIZE)
                        } else if (silenceBuffer.isNotEmpty()) {
                            val toSend = ShortArray(silenceBuffer.size) { silenceBuffer.removeFirst() }
                            sendChunked(session.socket, toSend, CHUNK_SIZE)
                        } else {
                            true
                        }
                    }
                }

                if (!sendOk) {
                    // Send failed — reconnect
                    session = tryReconnect(apiKey, onTranscript, audioBuffer, silenceBuffer)
                        ?: break
                    audioMode = AudioMode.CATCH_UP
                    modeStartMs = System.currentTimeMillis()
                    lastTranscriptionMs = System.currentTimeMillis()
                    consecutiveEmptyPolls = 0
                    continue
                }

                // Read transcriptions from the incoming queue
                var readCount = 0
                while (readCount < 20) {
                    val event = session.incomingEvents.poll() ?: break
                    readCount++
                    when (event) {
                        is LiveSocketEvent.Transcript -> {
                            lastTranscriptionMs = System.currentTimeMillis()
                            consecutiveEmptyPolls = 0
                            onTranscript(event.text)
                        }
                        is LiveSocketEvent.Error -> {
                            throw IOException(event.message)
                        }
                        is LiveSocketEvent.Closed -> {
                            // Server closed — reconnect
                            session = tryReconnect(apiKey, onTranscript, audioBuffer, silenceBuffer)
                                ?: throw IOException("Gemini Live reconnection failed.")
                            audioMode = AudioMode.CATCH_UP
                            modeStartMs = System.currentTimeMillis()
                            lastTranscriptionMs = System.currentTimeMillis()
                            consecutiveEmptyPolls = 0
                            break
                        }
                    }
                }
                if (readCount == 0) {
                    consecutiveEmptyPolls++
                }

                // Degradation detection: stalled connection
                val timeSinceTranscription = System.currentTimeMillis() - lastTranscriptionMs
                if (consecutiveEmptyPolls >= EMPTY_READ_CHECK_COUNT &&
                    timeSinceTranscription > NO_RESULT_THRESHOLD_MS
                ) {
                    session.socket.close(1000, "stalled")
                    session = tryReconnect(apiKey, onTranscript, audioBuffer, silenceBuffer)
                        ?: throw IOException("Gemini Live reconnection failed after stall.")
                    audioMode = AudioMode.CATCH_UP
                    modeStartMs = System.currentTimeMillis()
                    lastTranscriptionMs = System.currentTimeMillis()
                    consecutiveEmptyPolls = 0
                    continue
                }

                // If audio flow completed (mic stopped), exit
                if (collectJob.isCompleted) break

                delay(SEND_INTERVAL_MS)
            }
        } finally {
            collectJob.cancel()
            session.socket.close(1000, "SGT session finished")
        }
        } // coroutineScope
    }

    private data class LiveSession(
        val socket: WebSocket,
        val incomingEvents: LinkedBlockingDeque<LiveSocketEvent>,
    )

    private sealed class LiveSocketEvent {
        data class Transcript(val text: String) : LiveSocketEvent()
        data class Error(val message: String) : LiveSocketEvent()
        data object Closed : LiveSocketEvent()
    }

    private suspend fun connectAndSetup(
        apiKey: String,
        onTranscript: (String) -> Unit,
    ): LiveSession? {
        val events = LinkedBlockingDeque<LiveSocketEvent>()
        val setupReady = CompletableDeferred<Unit>()

        val request = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()

        val socket = httpClient.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    webSocket.send(buildSetupPayload())
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    handleMessage(text, events, setupReady)
                }

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                    handleMessage(bytes.utf8(), events, setupReady)
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    events.offer(LiveSocketEvent.Closed)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    events.offer(LiveSocketEvent.Closed)
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (!setupReady.isCompleted) {
                        setupReady.completeExceptionally(t)
                    }
                    events.offer(LiveSocketEvent.Closed)
                }
            },
        )

        return try {
            withTimeout(20_000) { setupReady.await() }
            LiveSession(socket, events)
        } catch (e: Throwable) {
            socket.close(1000, "setup failed")
            null
        }
    }

    private fun handleMessage(
        message: String,
        events: LinkedBlockingDeque<LiveSocketEvent>,
        setupReady: CompletableDeferred<Unit>,
    ) {
        if (message.contains("setupComplete")) {
            if (!setupReady.isCompleted) {
                setupReady.complete(Unit)
            }
            return
        }

        parseError(message)?.let { error ->
            events.offer(LiveSocketEvent.Error(error))
            return
        }

        parseInputTranscription(message)?.let { transcript ->
            events.offer(LiveSocketEvent.Transcript(transcript))
            return
        }
    }

    private suspend fun tryReconnect(
        apiKey: String,
        onTranscript: (String) -> Unit,
        audioBuffer: LinkedBlockingDeque<ShortArray>,
        silenceBuffer: MutableList<Short>,
    ): LiveSession? {
        // Drain pending audio into silence buffer for catchup replay
        while (true) {
            val chunk = audioBuffer.poll() ?: break
            for (s in chunk) silenceBuffer.add(s)
        }

        // Retry indefinitely until success or cancellation
        while (currentCoroutineContext().isActive) {
            // Drain any audio that arrived during reconnection attempt
            while (true) {
                val chunk = audioBuffer.poll() ?: break
                for (s in chunk) silenceBuffer.add(s)
            }

            val session = connectAndSetup(apiKey, onTranscript)
            if (session != null) {
                // Final drain before resuming
                while (true) {
                    val chunk = audioBuffer.poll() ?: break
                    for (s in chunk) silenceBuffer.add(s)
                }
                return session
            }
            delay(1_000)
        }
        return null
    }

    private fun sendChunked(socket: WebSocket, samples: ShortArray, chunkSize: Int): Boolean {
        var offset = 0
        while (offset < samples.size) {
            val end = minOf(offset + chunkSize, samples.size)
            val chunk = samples.copyOfRange(offset, end)
            if (!socket.send(buildAudioChunkPayload(chunk))) {
                return false
            }
            offset = end
        }
        return true
    }

    private fun buildSetupPayload(): String {
        val generationConfig = JSONObject()
            .put("responseModalities", JSONArray().put("AUDIO"))
            .put(
                "thinkingConfig",
                JSONObject().put("thinkingBudget", 0),
            )

        return JSONObject()
            .put(
                "setup",
                JSONObject()
                    .put("model", "models/$LIVE_MODEL")
                    .put("generationConfig", generationConfig)
                    .put("inputAudioTranscription", JSONObject()),
            )
            .toString()
    }

    private fun buildAudioChunkPayload(chunk: ShortArray): String {
        val bytes = ByteArray(chunk.size * 2)
        chunk.forEachIndexed { index, sample ->
            val byteIndex = index * 2
            bytes[byteIndex] = (sample.toInt() and 0xFF).toByte()
            bytes[byteIndex + 1] = ((sample.toInt() shr 8) and 0xFF).toByte()
        }
        val encoded = Base64.encodeToString(bytes, Base64.NO_WRAP)
        return JSONObject()
            .put(
                "realtimeInput",
                JSONObject().put(
                    "audio",
                    JSONObject()
                        .put("data", encoded)
                        .put("mimeType", "audio/pcm;rate=16000"),
                ),
            )
            .toString()
    }

    private fun parseInputTranscription(message: String): String? {
        return runCatching {
            JSONObject(message)
                .optJSONObject("serverContent")
                ?.optJSONObject("inputTranscription")
                ?.optString("text")
                ?.takeIf(String::isNotBlank)
        }.getOrNull()
    }

    private fun parseError(message: String): String? {
        return runCatching {
            JSONObject(message)
                .optJSONObject("error")
                ?.optString("message")
                ?.takeIf(String::isNotBlank)
        }.getOrNull()
    }

    private companion object {
        private const val TAG = "SGTGeminiLive"
        private const val LIVE_MODEL = "gemini-2.5-flash-native-audio-preview-12-2025"
        private const val LIVE_WS_ENDPOINT =
            "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
        private const val NORMAL_DURATION_MS = 20_000L
        private const val SILENCE_DURATION_MS = 2_000L
        private const val SAMPLES_PER_100MS = 1_600
        private const val CHUNK_SIZE = 1_600
        private const val SEND_INTERVAL_MS = 100L
        private const val EMPTY_READ_CHECK_COUNT = 50
        private const val NO_RESULT_THRESHOLD_MS = 8_000L
    }
}

class RealtimeTranslationClient(
    private val httpClient: OkHttpClient,
) {
    suspend fun streamTranslation(
        geminiApiKey: String,
        cerebrasApiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
        providerId: String,
        model: String,
        onDelta: (String) -> Unit,
    ): String = withContext(Dispatchers.IO) {
        val primary = TranslationProvider(providerId, model)

        // Check if primary provider is available (has API key if needed)
        val primaryAvailable = isProviderAvailable(primary.id, geminiApiKey, cerebrasApiKey)

        if (primaryAvailable) {
            runCatching {
                streamWithProvider(
                    provider = primary,
                    geminiApiKey = geminiApiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    request = request,
                    targetLanguage = targetLanguage,
                    onDelta = onDelta,
                )
                return@withContext primary.id
            }
        }

        // Primary failed or unavailable — try fallback
        val fallback = fallbackProvider(primary.id, geminiApiKey, cerebrasApiKey)
        streamWithProvider(
            provider = fallback,
            geminiApiKey = geminiApiKey,
            cerebrasApiKey = cerebrasApiKey,
            request = request,
            targetLanguage = targetLanguage,
            onDelta = onDelta,
        )
        fallback.id
    }

    private fun streamWithProvider(
        provider: TranslationProvider,
        geminiApiKey: String,
        cerebrasApiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
        onDelta: (String) -> Unit,
    ) {
        when (provider.id) {
            PROVIDER_GTX -> {
                val text = translateWithGoogleGtx(
                    text = request.chunk,
                    targetLanguage = targetLanguage,
                ) ?: error("GTX translation failed.")
                onDelta(text)
            }

            PROVIDER_CEREBRAS -> {
                val apiKey = cerebrasApiKey.takeIf { it.isNotBlank() }
                    ?: error("Add your Cerebras API key before using Cerebras translation.")
                streamChatCompletion(
                    endpoint = "https://api.cerebras.ai/v1/chat/completions",
                    apiKey = apiKey,
                    model = provider.model,
                    messages = cerebrasMessages(request, targetLanguage),
                    onDelta = onDelta,
                )
            }

            else -> {
                val apiKey = geminiApiKey.takeIf { it.isNotBlank() }
                    ?: error("Add your Gemini API key before using Gemma translation.")
                streamChatCompletion(
                    endpoint = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
                    apiKey = apiKey,
                    model = provider.model,
                    messages = gemmaMessages(request, targetLanguage),
                    onDelta = onDelta,
                )
            }
        }
    }

    private fun streamChatCompletion(
        endpoint: String,
        apiKey: String,
        model: String,
        messages: JSONArray,
        onDelta: (String) -> Unit,
    ) {
        val requestBody = JSONObject()
            .put("model", model)
            .put("messages", messages)
            .put("stream", true)
            .put("max_tokens", 512)
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)

        val httpRequest = Request.Builder()
            .url(endpoint)
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Translation request failed with ${response.code}")
            }
            val body = response.body ?: throw IOException("Translation response body was empty.")
            var emittedText = ""
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    val line = rawLine.trim()
                    if (!line.startsWith("data: ")) {
                        return@forEach
                    }
                    val payload = line.removePrefix("data: ").trim()
                    if (payload.isBlank() || payload == "[DONE]") {
                        return@forEach
                    }
                    val candidateText = extractStreamText(payload)
                    if (candidateText.isBlank()) {
                        return@forEach
                    }
                    val delta = when {
                        candidateText.startsWith(emittedText) -> candidateText.removePrefix(emittedText)
                        emittedText.startsWith(candidateText) -> ""
                        else -> candidateText
                    }
                    if (delta.isNotBlank()) {
                        emittedText += delta
                        onDelta(delta)
                    }
                }
            }
        }
    }

    private fun extractStreamText(payload: String): String {
        return try {
            val root = JSONObject(payload)
            val choices = root.optJSONArray("choices") ?: JSONArray()
            val choice = choices.optJSONObject(0) ?: return ""
            choice.optJSONObject("delta")?.optString("content").orEmpty()
        } catch (_error: JSONException) {
            ""
        }
    }

    private fun gemmaMessages(
        request: TranslationRequest,
        targetLanguage: String,
    ): JSONArray {
        val prompt = buildPrompt(request, targetLanguage)
        val messages = JSONArray()
        request.history.forEach { entry ->
            messages.put(
                JSONObject()
                    .put("role", "user")
                    .put("content", "Translate to $targetLanguage:\n${entry.source}"),
            )
            messages.put(
                JSONObject()
                    .put("role", "assistant")
                    .put("content", entry.translation),
            )
        }
        messages.put(
            JSONObject()
                .put("role", "user")
                .put("content", prompt),
        )
        return messages
    }

    private fun cerebrasMessages(
        request: TranslationRequest,
        targetLanguage: String,
    ): JSONArray {
        val messages = JSONArray()
        messages.put(
            JSONObject()
                .put("role", "system")
                .put(
                    "content",
                    "You are a professional translator. Translate text to $targetLanguage to append suitably to the context. Output ONLY the translation, nothing else.",
                ),
        )
        request.history.forEach { entry ->
            messages.put(
                JSONObject()
                    .put("role", "user")
                    .put("content", "Translate to $targetLanguage:\n${entry.source}"),
            )
            messages.put(
                JSONObject()
                    .put("role", "assistant")
                    .put("content", entry.translation),
            )
        }
        messages.put(
            JSONObject()
                .put("role", "user")
                .put("content", "Translate to $targetLanguage:\n${request.chunk}"),
        )
        return messages
    }

    private fun buildPrompt(
        request: TranslationRequest,
        targetLanguage: String,
    ): String {
        return buildString {
            append("You are a professional translator. Translate the next live transcript chunk to ")
            append(targetLanguage)
            append(". Output only the translation text that should be appended to the running result.\n\n")
            if (request.history.isNotEmpty()) {
                append("Recent committed context:\n")
                request.history.forEach { entry ->
                    append("Source: ")
                    append(entry.source)
                    append('\n')
                    append("Translation: ")
                    append(entry.translation)
                    append("\n\n")
                }
            }
            append("Translate to ")
            append(targetLanguage)
            append(":\n")
            append(request.chunk)
        }
    }

    private fun translateWithGoogleGtx(
        text: String,
        targetLanguage: String,
    ): String? {
        val targetCode = LanguageCatalog.codeForName(targetLanguage).lowercase()
        val request = Request.Builder()
            .url(
                "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=$targetCode&dt=t&q=${java.net.URLEncoder.encode(text, "UTF-8")}",
            )
            .header("User-Agent", "Mozilla/5.0")
            .build()
        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                return null
            }
            val payload = response.body?.string().orEmpty()
            val sentences = JSONArray(payload).optJSONArray(0) ?: return null
            return buildString {
                for (index in 0 until sentences.length()) {
                    append(sentences.optJSONArray(index)?.optString(0).orEmpty())
                }
            }.ifBlank { null }
        }
    }

    private fun isProviderAvailable(
        providerId: String,
        geminiApiKey: String,
        cerebrasApiKey: String,
    ): Boolean {
        return when (providerId) {
            PROVIDER_GTX -> true // always available, no API key needed
            PROVIDER_CEREBRAS -> cerebrasApiKey.isNotBlank()
            else -> geminiApiKey.isNotBlank() // google-gemma
        }
    }

    private fun fallbackProvider(
        providerId: String,
        geminiApiKey: String,
        cerebrasApiKey: String,
    ): TranslationProvider {
        // Match Windows: cerebras ↔ gtx, gemma → random(cerebras, gtx)
        // But only pick providers that are actually available
        val candidates = when (providerId) {
            PROVIDER_CEREBRAS -> listOf(
                TranslationProvider(PROVIDER_GTX, "google-translate-gtx"),
            )
            PROVIDER_GTX -> listOf(
                TranslationProvider(PROVIDER_CEREBRAS, "gpt-oss-120b"),
                TranslationProvider(PROVIDER_GTX, "google-translate-gtx"), // self-retry as last resort
            )
            else -> listOf(
                TranslationProvider(PROVIDER_CEREBRAS, "gpt-oss-120b"),
                TranslationProvider(PROVIDER_GTX, "google-translate-gtx"),
            )
        }
        return candidates.firstOrNull { isProviderAvailable(it.id, geminiApiKey, cerebrasApiKey) }
            ?: TranslationProvider(PROVIDER_GTX, "google-translate-gtx") // GTX always works
    }

    private companion object {
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
        private const val PROVIDER_CEREBRAS = "cerebras-oss"
        private const val PROVIDER_GTX = "google-gtx"
    }

    private data class TranslationProvider(
        val id: String,
        val model: String,
    )
}
