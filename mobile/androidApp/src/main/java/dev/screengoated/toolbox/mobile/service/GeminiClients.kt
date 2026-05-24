package dev.screengoated.toolbox.mobile.service

import android.util.Base64
import android.util.Log
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.streamGeminiLiveText
import dev.screengoated.toolbox.mobile.shared.live.LiveTranslationModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.TranslationRequest
import dev.screengoated.toolbox.mobile.shared.live.TranslationResponse
import dev.screengoated.toolbox.mobile.shared.live.TranslationPatch
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
        model: String,
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
        var session = connectAndSetup(apiKey, model)
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
                            val toSend = ShortArray(doubleChunk) { silenceBuffer.removeAt(0) }
                            sendChunked(session.socket, toSend, CHUNK_SIZE)
                        } else if (silenceBuffer.isNotEmpty()) {
                            val toSend = ShortArray(silenceBuffer.size) { silenceBuffer.removeAt(0) }
                            sendChunked(session.socket, toSend, CHUNK_SIZE)
                        } else {
                            true
                        }
                    }
                }

                if (!sendOk) {
                    // Send failed — reconnect
                    session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer)
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
                            session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer)
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
                    session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer)
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
        model: String,
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
                    webSocket.send(buildSetupPayload(model))
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
        model: String,
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

            val session = connectAndSetup(apiKey, model)
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

    private fun buildSetupPayload(model: String): String {
        val generationConfig = JSONObject()
            .put("responseModalities", JSONArray().put("AUDIO"))
            .put("mediaResolution", "MEDIA_RESOLUTION_LOW")
            .put("thinkingConfig", JSONObject().put("thinkingBudget", 0))

        val setup = JSONObject()
            .put(
                "setup",
                JSONObject()
                    .put("model", "models/$model")
                    .put("generationConfig", generationConfig)
                    .put("inputAudioTranscription", JSONObject()),
            )

        return setup.toString()
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
    suspend fun translate(
        geminiApiKey: String,
        cerebrasApiKey: String,
        groqApiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
        providerId: String,
        llmChain: List<String>,
    ): TranslationExecutionResult = withContext(Dispatchers.IO) {
        val primaryId = providerId

        runCatching {
            val response = dispatchProvider(
                providerId = primaryId,
                geminiApiKey = geminiApiKey,
                cerebrasApiKey = cerebrasApiKey,
                groqApiKey = groqApiKey,
                request = request,
                targetLanguage = targetLanguage,
                llmChain = llmChain,
            )
            return@withContext TranslationExecutionResult(primaryId, response)
        }

        val fallbackId = if (primaryId == PROVIDER_GTX) PROVIDER_LLM else PROVIDER_GTX
        val response = dispatchProvider(
            providerId = fallbackId,
            geminiApiKey = geminiApiKey,
            cerebrasApiKey = cerebrasApiKey,
            groqApiKey = groqApiKey,
            request = request,
            targetLanguage = targetLanguage,
            llmChain = llmChain,
        )
        TranslationExecutionResult(fallbackId, response)
    }

    private suspend fun dispatchProvider(
        providerId: String,
        geminiApiKey: String,
        cerebrasApiKey: String,
        groqApiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
        llmChain: List<String>,
    ): TranslationResponse {
        Log.d(
            TRANSLATION_TAG,
            "request provider=$providerId range=${request.sourceStart}-${request.sourceEnd} finalize=${request.bytesToCommit} draft=${request.draftSource.length}",
        )
        if (providerId == PROVIDER_GTX) {
            return translateWithGoogleGtx(request, targetLanguage)
        }
        return translateWithLlmChain(
            chainModelIds = llmChain,
            geminiApiKey = geminiApiKey,
            cerebrasApiKey = cerebrasApiKey,
            groqApiKey = groqApiKey,
            request = request,
            targetLanguage = targetLanguage,
        )
    }

    private suspend fun translateWithLlmChain(
        chainModelIds: List<String>,
        geminiApiKey: String,
        cerebrasApiKey: String,
        groqApiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        var lastError: Throwable? = null
        for (modelId in chainModelIds) {
            val descriptor = PresetModelCatalog.getById(modelId) ?: continue
            val attempt = runCatching {
                when (descriptor.provider) {
                    PresetModelProvider.CEREBRAS -> {
                        val key = cerebrasApiKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithCerebras(
                            endpoint = "https://api.cerebras.ai/v1/chat/completions",
                            apiKey = key,
                            model = descriptor.fullName,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    PresetModelProvider.GOOGLE -> {
                        val key = geminiApiKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithGemini(
                            endpoint = "https://generativelanguage.googleapis.com/v1beta/models/${descriptor.fullName}:generateContent",
                            apiKey = key,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    PresetModelProvider.GEMINI_LIVE -> {
                        val key = geminiApiKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithGeminiLive(
                            descriptor = descriptor,
                            apiKey = key,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    PresetModelProvider.GROQ -> {
                        val key = groqApiKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithGroq(
                            apiKey = key,
                            model = descriptor.fullName,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    else -> null
                }
            }
            val result = attempt.getOrElse {
                lastError = it
                null
            }
            if (result != null) {
                return result
            }
        }
        throw lastError ?: IOException("No LLM provider in the priority chain produced a translation.")
    }

    private suspend fun translateWithGeminiLive(
        descriptor: PresetModelDescriptor,
        apiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val text = httpClient.streamGeminiLiveText(
            model = descriptor,
            apiKey = apiKey,
            prompt = "",
            inputText = buildStructuredPrompt(request, targetLanguage),
            onChunk = {},
        )
        return parseTranslationResponse(text, request)
    }

    private fun translateWithCerebras(
        endpoint: String,
        apiKey: String,
        model: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        // Cerebras reasoning models reject strict json_schema; rely on prompt-driven JSON.
        val isReasoning = model.contains("gpt-oss") || model.contains("zai-glm")
        val payload = JSONObject()
            .put("model", model)
            .put("messages", cerebrasMessages(request, targetLanguage))
            .put("stream", false)
            .put("max_tokens", 512)
        if (!isReasoning) {
            payload.put("response_format", cerebrasResponseFormat())
        }
        val requestBody = payload.toString().toRequestBody(JSON_MEDIA_TYPE)

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
            val body = response.body?.string().orEmpty()
            val root = JSONObject(body)
            val jsonText = root.optJSONArray("choices")
                ?.optJSONObject(0)
                ?.optJSONObject("message")
                ?.optString("content")
                .orEmpty()
            return parseTranslationResponse(jsonText, request)
        }
    }

    private fun translateWithGroq(
        apiKey: String,
        model: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val requestBody = JSONObject()
            .put("model", model)
            .put("messages", cerebrasMessages(request, targetLanguage))
            .put("stream", false)
            .put("max_tokens", 512)
            .put("response_format", JSONObject().put("type", "json_object"))
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)

        val httpRequest = Request.Builder()
            .url("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Groq translation request failed with ${response.code}")
            }
            val body = response.body?.string().orEmpty()
            val root = JSONObject(body)
            val jsonText = root.optJSONArray("choices")
                ?.optJSONObject(0)
                ?.optJSONObject("message")
                ?.optString("content")
                .orEmpty()
            return parseTranslationResponse(jsonText, request)
        }
    }

    private fun translateWithGemini(
        endpoint: String,
        apiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val requestBody = JSONObject()
            .put(
                "contents",
                JSONArray().put(
                    JSONObject()
                        .put("role", "user")
                        .put(
                            "parts",
                            JSONArray().put(
                                JSONObject().put("text", buildStructuredPrompt(request, targetLanguage)),
                            ),
                        ),
                ),
            )
            .put(
                "generationConfig",
                JSONObject().put("responseMimeType", "application/json"),
            )
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)

        val httpRequest = Request.Builder()
            .url(endpoint)
            .header("x-goog-api-key", apiKey)
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Gemini translation request failed with ${response.code}")
            }
            val body = response.body?.string().orEmpty()
            val root = JSONObject(body)
            val parts = root.optJSONArray("candidates")
                ?.optJSONObject(0)
                ?.optJSONObject("content")
                ?.optJSONArray("parts")
                ?: throw IOException("Gemini translation response body was empty.")
            val jsonText = buildString {
                for (index in 0 until parts.length()) {
                    append(parts.optJSONObject(index)?.optString("text").orEmpty())
                }
            }
            return parseTranslationResponse(jsonText, request)
        }
    }

    private fun translateWithGoogleGtx(
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val patches = mutableListOf<TranslationPatch>()
        if (request.finalizedSource.isNotBlank()) {
            val translated = translateWithGoogleGtxText(
                text = request.finalizedSource,
                targetLanguage = targetLanguage,
            ) ?: error("GTX finalized translation failed.")
            patches += TranslationPatch(
                sourceStart = request.sourceStart,
                sourceEnd = request.finalizedSourceEnd,
                state = "final",
                translation = translated,
            )
        }
        if (request.draftSource.isNotBlank()) {
            val translated = translateWithGoogleGtxText(
                text = request.draftSource,
                targetLanguage = targetLanguage,
            ) ?: error("GTX draft translation failed.")
            patches += TranslationPatch(
                sourceStart = request.draftSourceStart,
                sourceEnd = request.sourceEnd,
                state = "draft",
                translation = translated,
            )
        }
        return validateTranslationResponse(TranslationResponse(patches), request)
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
                    "You translate live transcript windows into JSON source patches. Respond with JSON only.",
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
                .put("content", buildStructuredPrompt(request, targetLanguage)),
        )
        return messages
    }

    private fun buildStructuredPrompt(
        request: TranslationRequest,
        targetLanguage: String,
    ): String {
        val history = JSONArray().apply {
            request.history.forEach { entry ->
                put(
                    JSONObject()
                        .put("source", entry.source)
                        .put("translation", entry.translation),
                )
            }
        }
        val expectedPatches = JSONArray().apply {
            if (request.finalizedSource.isNotBlank()) {
                put(
                    JSONObject()
                        .put("sourceStart", request.sourceStart)
                        .put("sourceEnd", request.finalizedSourceEnd)
                        .put("state", "final"),
                )
            }
            if (request.draftSource.isNotBlank()) {
                put(
                    JSONObject()
                        .put("sourceStart", request.draftSourceStart)
                        .put("sourceEnd", request.sourceEnd)
                        .put("state", "draft"),
                )
            }
        }
        val window = JSONObject()
            .put("sourceStart", request.sourceStart)
            .put("sourceEnd", request.sourceEnd)
            .put("pendingSource", request.pendingSource)
            .put("finalizedSource", request.finalizedSource)
            .put("draftSource", request.draftSource)
            .put("previousDraftTranslation", request.previousDraftTranslation)

        return buildString {
            append("You are a professional live translator.\n")
            append("Translate only the provided source window into ")
            append(targetLanguage)
            append(".\n")
            append("Return JSON with a single key named patches.\n")
            append("Each patch must keep the exact sourceStart/sourceEnd values from expectedPatches.\n")
            append("Use state=\"final\" for the finalized source span and state=\"draft\" for the trailing unfinished span.\n")
            append("Do not add commentary, markdown, or extra keys.\n\n")
            append("Recent committed context:\n")
            append(history.toString())
            append("\n\n")
            append("Current source window:\n")
            append(window.toString())
            append("\n\n")
            append("Expected patches:\n")
            append(expectedPatches.toString())
        }
    }

    private fun cerebrasResponseFormat(): JSONObject {
        val patchSchema = JSONObject()
            .put("type", "object")
            .put(
                "properties",
                JSONObject()
                    .put("sourceStart", JSONObject().put("type", "integer"))
                    .put("sourceEnd", JSONObject().put("type", "integer"))
                    .put(
                        "state",
                        JSONObject()
                            .put("type", "string")
                            .put("enum", JSONArray().put("final").put("draft")),
                    )
                    .put("translation", JSONObject().put("type", "string")),
            )
            .put(
                "required",
                JSONArray()
                    .put("sourceStart")
                    .put("sourceEnd")
                    .put("state")
                    .put("translation"),
            )
            .put("additionalProperties", false)

        val schema = JSONObject()
            .put("type", "object")
            .put(
                "properties",
                JSONObject().put(
                    "patches",
                    JSONObject()
                        .put("type", "array")
                        .put("items", patchSchema),
                ),
            )
            .put("required", JSONArray().put("patches"))
            .put("additionalProperties", false)

        return JSONObject()
            .put("type", "json_schema")
            .put(
                "json_schema",
                JSONObject()
                    .put("name", "live_translate_patches")
                    .put("strict", true)
                    .put("schema", schema),
            )
    }

    private fun parseTranslationResponse(
        payload: String,
        request: TranslationRequest,
    ): TranslationResponse {
        if (payload.isBlank()) {
            throw IOException("Translation response payload was empty.")
        }
        try {
            val root = JSONObject(payload)
            val patchesJson = root.optJSONArray("patches")
                ?: throw IOException("Translation response did not include patches.")
            val patches = buildList {
                for (index in 0 until patchesJson.length()) {
                    val patch = patchesJson.optJSONObject(index) ?: continue
                    add(
                        TranslationPatch(
                            sourceStart = patch.optInt("sourceStart", Int.MIN_VALUE),
                            sourceEnd = patch.optInt("sourceEnd", Int.MIN_VALUE),
                            state = patch.optString("state"),
                            translation = patch.optString("translation"),
                        ),
                    )
                }
            }
            return validateTranslationResponse(TranslationResponse(patches), request)
        } catch (error: JSONException) {
            throw IOException("Translation response was not valid JSON.", error)
        }
    }

    private fun validateTranslationResponse(
        response: TranslationResponse,
        request: TranslationRequest,
    ): TranslationResponse {
        val expectedPatches = buildList<Triple<Int, Int, String>> {
            if (request.finalizedSource.isNotBlank()) {
                add(Triple(request.sourceStart, request.finalizedSourceEnd, "final"))
            }
            if (request.draftSource.isNotBlank()) {
                add(Triple(request.draftSourceStart, request.sourceEnd, "draft"))
            }
        }
        val normalized = mutableListOf<TranslationPatch>()
        expectedPatches.forEach { expected ->
            val patch = response.patches.firstOrNull { candidate ->
                candidate.sourceStart == expected.first &&
                    candidate.sourceEnd == expected.second &&
                    candidate.state == expected.third &&
                    candidate.translation.isNotBlank()
            }
            if (patch != null) {
                normalized += patch.copy(translation = patch.translation.trim())
                return@forEach
            }

            if (expected.third == "draft" && !request.requiresDraftTranslation()) {
                normalized += TranslationPatch(
                    sourceStart = expected.first,
                    sourceEnd = expected.second,
                    state = expected.third,
                    translation = request.fallbackDraftTranslation(),
                )
                return@forEach
            }

            throw IOException(
                "Translation response missing expected ${expected.third} patch ${expected.first}-${expected.second}.",
            )
        }
        return TranslationResponse(normalized)
    }

    private fun translateWithGoogleGtxText(
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

    private companion object {
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
        private val PROVIDER_LLM = LiveTranslationModelCatalog.PROVIDER_LLM
        private val PROVIDER_GTX = LiveTranslationModelCatalog.PROVIDER_GTX
        private const val TRANSLATION_TAG = "LiveTranslate"
    }

    data class TranslationExecutionResult(
        val providerId: String,
        val response: TranslationResponse,
    )
}
