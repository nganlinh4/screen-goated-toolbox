package dev.screengoated.toolbox.mobile.service

import android.util.Base64
import android.util.Log
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.shared.live.TranslationRequest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
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
import kotlin.random.Random

class GeminiLiveSocketClient(
    private val httpClient: OkHttpClient,
) {
    suspend fun runSession(
        apiKey: String,
        audioChunks: Flow<ShortArray>,
        onTranscript: (String) -> Unit,
    ) {
        val setupReady = CompletableDeferred<Unit>()
        val sessionClosed = CompletableDeferred<Unit>()
        var fatalError: Throwable? = null
        var transcriptEvents = 0
        var outboundChunks = 0
        var nonTranscriptServerMessages = 0

        val request = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()

        val socket = httpClient.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    Log.d(TAG, "Gemini Live socket open; sending setup")
                    webSocket.send(buildSetupPayload())
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    handleServerMessage(text)
                }

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                    handleServerMessage(bytes.utf8())
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    Log.d(TAG, "Gemini Live socket closing code=$code reason=$reason")
                    if (!sessionClosed.isCompleted) {
                        sessionClosed.complete(Unit)
                    }
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    Log.d(TAG, "Gemini Live socket closed code=$code reason=$reason")
                    if (!sessionClosed.isCompleted) {
                        sessionClosed.complete(Unit)
                    }
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    Log.e(TAG, "Gemini Live socket failure", t)
                    fatalError = t
                    if (!setupReady.isCompleted) {
                        setupReady.completeExceptionally(t)
                    }
                    if (!sessionClosed.isCompleted) {
                        sessionClosed.complete(Unit)
                    }
                }

                private fun handleServerMessage(message: String) {
                    if (message.contains("setupComplete")) {
                        Log.d(TAG, "Gemini Live setup complete")
                        if (!setupReady.isCompleted) {
                            setupReady.complete(Unit)
                        }
                        return
                    }

                    parseError(message)?.let { error ->
                        val throwable = IOException(error)
                        Log.e(TAG, "Gemini Live server error: $error")
                        fatalError = throwable
                        if (!setupReady.isCompleted) {
                            setupReady.completeExceptionally(throwable)
                        }
                        if (!sessionClosed.isCompleted) {
                            sessionClosed.complete(Unit)
                        }
                        return
                    }

                    parseInputTranscription(message)?.let { transcript ->
                        transcriptEvents += 1
                        Log.d(TAG, "Transcript[$transcriptEvents]: ${transcript.take(120)}")
                        onTranscript(transcript)
                        return
                    }

                    if (message.contains("\"serverContent\"") && nonTranscriptServerMessages < 5) {
                        nonTranscriptServerMessages += 1
                        Log.d(TAG, "Server message without transcript: ${message.take(240)}")
                    }
                }
            },
        )

        try {
            withTimeout(20_000) { setupReady.await() }
            audioChunks.collect { chunk ->
                fatalError?.let { throw it }
                if (sessionClosed.isCompleted) {
                    throw IOException("Gemini Live session closed.")
                }
                outboundChunks += 1
                val accepted = socket.send(buildAudioChunkPayload(chunk))
                if (!accepted) {
                    throw IOException("Gemini Live rejected an audio frame.")
                }
                if (outboundChunks == 1 || outboundChunks % 20 == 0) {
                    Log.d(TAG, "Sent audio chunk #$outboundChunks samples=${chunk.size}")
                }
            }
        } finally {
            socket.close(1000, "SGT session finished")
            if (!sessionClosed.isCompleted) {
                sessionClosed.complete(Unit)
            }
            fatalError?.let { throw it }
        }
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
        runCatching {
            streamWithProvider(
                provider = primary,
                geminiApiKey = geminiApiKey,
                cerebrasApiKey = cerebrasApiKey,
                request = request,
                targetLanguage = targetLanguage,
                onDelta = onDelta,
            )
            primary.id
        }.getOrElse {
            val fallback = fallbackProvider(primary.id)
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
                    .put("role", "assistant")
                    .put("content", "Source: ${entry.source}\nTranslation: ${entry.translation}"),
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
                    .put("role", "assistant")
                    .put("content", "Source: ${entry.source}\nTranslation: ${entry.translation}"),
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

    private fun fallbackProvider(providerId: String): TranslationProvider {
        return when (providerId) {
            PROVIDER_CEREBRAS -> TranslationProvider(PROVIDER_GTX, "google-translate-gtx")
            PROVIDER_GTX -> TranslationProvider(PROVIDER_CEREBRAS, "gpt-oss-120b")
            else -> {
                if (Random.nextBoolean()) {
                    TranslationProvider(PROVIDER_CEREBRAS, "gpt-oss-120b")
                } else {
                    TranslationProvider(PROVIDER_GTX, "google-translate-gtx")
                }
            }
        }
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
