package dev.screengoated.toolbox.mobile.preset

import android.util.Log
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.net.URLEncoder
import kotlin.coroutines.coroutineContext

data class ApiKeys(
    val geminiKey: String = "",
    val cerebrasKey: String = "",
    val groqKey: String = "",
    val openRouterKey: String = "",
    val ollamaBaseUrl: String = "",
)

class TextApiClient(private val httpClient: OkHttpClient) {

    suspend fun executeStreaming(
        modelId: String,
        prompt: String,
        inputText: String,
        apiKeys: ApiKeys,
        uiLanguage: String,
        searchLabel: String?,
        onChunk: (String) -> Unit,
    ): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val model = resolveModel(modelId)
            when (model.provider) {
                PresetModelProvider.GOOGLE -> streamGemini(
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    apiKey = apiKeys.geminiKey,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                )
                PresetModelProvider.CEREBRAS -> streamCerebras(
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    apiKey = apiKeys.cerebrasKey,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                )
                PresetModelProvider.GROQ -> {
                    if (model.fullName.startsWith("groq/compound")) {
                        runGroqCompound(
                            apiKey = apiKeys.groqKey,
                            model = model,
                            prompt = prompt,
                            inputText = inputText,
                            searchLabel = searchLabel,
                            onChunk = onChunk,
                        )
                    } else {
                        streamOpenAiCompatible(
                            endpoint = GROQ_ENDPOINT,
                            apiKey = apiKeys.groqKey,
                            providerName = "Groq",
                            model = model,
                            prompt = prompt,
                            inputText = inputText,
                            uiLanguage = uiLanguage,
                            onChunk = onChunk,
                        )
                    }
                }
                PresetModelProvider.OPENROUTER -> streamOpenAiCompatible(
                    endpoint = OPENROUTER_ENDPOINT,
                    apiKey = apiKeys.openRouterKey,
                    providerName = "OpenRouter",
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                )
                PresetModelProvider.GOOGLE_GTX -> translateGoogleGtx(
                    inputText = inputText,
                    prompt = prompt,
                    onChunk = onChunk,
                )
                PresetModelProvider.OLLAMA -> streamOllama(
                    baseUrl = apiKeys.ollamaBaseUrl,
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                )
                PresetModelProvider.GEMINI_LIVE ->
                    throw IOException("PROVIDER_NOT_READY:gemini-live")
                else ->
                    throw IOException("Unsupported text provider: ${model.provider.name.lowercase()}")
            }
        }
    }

    private fun resolveModel(modelId: String): PresetModelDescriptor {
        return requireNotNull(PresetModelCatalog.getById(modelId)) {
            "Unknown model config: $modelId"
        }
    }

    private suspend fun streamGemini(
        model: PresetModelDescriptor,
        prompt: String,
        inputText: String,
        apiKey: String,
        uiLanguage: String,
        onChunk: (String) -> Unit,
    ): String {
        if (apiKey.isBlank()) throw IOException("NO_API_KEY:gemini")

        val payload = JSONObject().put(
            "contents",
            JSONArray().put(
                JSONObject()
                    .put("role", "user")
                    .put(
                        "parts",
                        JSONArray().put(
                            JSONObject().put("text", "$prompt\n\n$inputText"),
                        ),
                    ),
            ),
        )

        PresetModelCatalog.geminiThinkingConfig(model.fullName)?.let { thinking ->
            payload.put(
                "generationConfig",
                JSONObject().put(
                    "thinkingConfig",
                    JSONObject(thinking),
                ),
            )
        }

        if (PresetModelCatalog.supportsSearchByName(model.fullName)) {
            payload.put(
                "tools",
                JSONArray()
                    .put(JSONObject().put("url_context", JSONObject()))
                    .put(JSONObject().put("google_search", JSONObject())),
            )
        }

        val url = "$GEMINI_ENDPOINT/${model.fullName}:streamGenerateContent?alt=sse"
        val requestBody = payload.toString().toRequestBody(JSON_MEDIA_TYPE)
        val request = Request.Builder()
            .url(url)
            .header("x-goog-api-key", apiKey)
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        val fullContent = StringBuilder()
        var thinkingShown = false
        var contentStarted = false

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("Gemini request failed with $code")
            }

            val body = response.body ?: throw IOException("Gemini response body was empty.")
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    coroutineContext.ensureActive()
                    val line = rawLine.trim()
                    if (!line.startsWith("data: ")) return@forEach
                    val data = line.removePrefix("data: ").trim()
                    if (data.isBlank() || data == "[DONE]") return@forEach

                    val delta = extractGeminiDelta(data)
                    if (delta.reasoning && !thinkingShown && !contentStarted) {
                        onChunk(thinkingLabel(uiLanguage))
                        thinkingShown = true
                    }
                    if (delta.content.isNotEmpty()) {
                        if (!contentStarted && thinkingShown) {
                            contentStarted = true
                            fullContent.append(delta.content)
                            onChunk("$WIPE_SIGNAL$fullContent")
                        } else {
                            contentStarted = true
                            fullContent.append(delta.content)
                            onChunk(delta.content)
                        }
                    }
                }
            }
        }

        return fullContent.toString()
    }

    private fun extractGeminiDelta(payload: String): GeminiDelta {
        return try {
            val root = JSONObject(payload)
            val candidates = root.optJSONArray("candidates") ?: return GeminiDelta()
            val candidate = candidates.optJSONObject(0) ?: return GeminiDelta()
            val parts = candidate
                .optJSONObject("content")
                ?.optJSONArray("parts") ?: return GeminiDelta()

            var content = ""
            var reasoning = false
            for (index in 0 until parts.length()) {
                val part = parts.optJSONObject(index) ?: continue
                if (part.optBoolean("thought", false)) {
                    reasoning = true
                    continue
                }
                content += part.optString("text", "")
            }
            GeminiDelta(content = content, reasoning = reasoning)
        } catch (_: JSONException) {
            GeminiDelta()
        }
    }

    private suspend fun streamOpenAiCompatible(
        endpoint: String,
        apiKey: String,
        providerName: String,
        model: PresetModelDescriptor,
        prompt: String,
        inputText: String,
        uiLanguage: String,
        onChunk: (String) -> Unit,
    ): String {
        if (apiKey.isBlank()) throw IOException("NO_API_KEY:${providerName.lowercase()}")

        val payload = JSONObject()
            .put("model", model.fullName)
            .put(
                "messages",
                JSONArray().put(
                    JSONObject()
                        .put("role", "user")
                        .put("content", "$prompt\n\n$inputText"),
                ),
            )
            .put("stream", true)

        val request = Request.Builder()
            .url(endpoint)
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(payload.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()

        val fullContent = StringBuilder()
        var thinkingShown = false
        var contentStarted = false
        val reasoningFallback = model.provider == PresetModelProvider.CEREBRAS &&
            (model.fullName.contains("gpt-oss") || model.fullName.contains("zai-glm"))

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("$providerName request failed with $code")
            }

            val body = response.body ?: throw IOException("$providerName response body was empty.")
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    coroutineContext.ensureActive()
                    val line = rawLine.trim()
                    if (!line.startsWith("data: ")) return@forEach
                    val data = line.removePrefix("data: ").trim()
                    if (data.isBlank() || data == "[DONE]") return@forEach

                    val delta = extractOpenAiDelta(data)
                    if ((delta.reasoning.isNotEmpty() || reasoningFallback) && !thinkingShown && !contentStarted) {
                        onChunk(thinkingLabel(uiLanguage))
                        thinkingShown = true
                    }
                    if (delta.content.isNotEmpty()) {
                        if (!contentStarted && thinkingShown) {
                            contentStarted = true
                            fullContent.append(delta.content)
                            onChunk("$WIPE_SIGNAL$fullContent")
                        } else {
                            contentStarted = true
                            fullContent.append(delta.content)
                            onChunk(delta.content)
                        }
                    }
                }
            }
        }

        return fullContent.toString()
    }

    private suspend fun streamCerebras(
        model: PresetModelDescriptor,
        prompt: String,
        inputText: String,
        apiKey: String,
        uiLanguage: String,
        onChunk: (String) -> Unit,
    ): String {
        if (apiKey.isBlank()) throw IOException("NO_API_KEY:cerebras")

        val payload = JSONObject()
            .put("model", model.fullName)
            .put(
                "messages",
                JSONArray().put(
                    JSONObject()
                        .put("role", "user")
                        .put("content", "$prompt\n\n$inputText"),
                ),
            )
            .put("stream", true)

        val request = Request.Builder()
            .url(CEREBRAS_ENDPOINT)
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(payload.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()

        val fullContent = StringBuilder()
        var thinkingShown = false
        var contentStarted = false
        val reasoningFallback =
            model.fullName.contains("gpt-oss") || model.fullName.contains("zai-glm")

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                val errorBody = response.body?.string().orEmpty()
                Log.e(
                    "TextApiClient",
                    "Cerebras request failed code=$code model=${model.fullName} body=$errorBody",
                )
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("Cerebras request failed with $code")
            }

            val body = response.body ?: throw IOException("Cerebras response body was empty.")
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    coroutineContext.ensureActive()
                    val line = rawLine.trim()
                    if (!line.startsWith("data: ")) return@forEach
                    val data = line.removePrefix("data: ").trim()
                    if (data.isBlank() || data == "[DONE]") return@forEach

                    val delta = extractOpenAiDelta(data)
                    if ((delta.reasoning.isNotEmpty() || reasoningFallback) &&
                        !thinkingShown &&
                        !contentStarted
                    ) {
                        onChunk(thinkingLabel(uiLanguage))
                        thinkingShown = true
                    }
                    if (delta.content.isNotEmpty()) {
                        if (!contentStarted && thinkingShown) {
                            contentStarted = true
                            fullContent.append(delta.content)
                            onChunk("$WIPE_SIGNAL$fullContent")
                        } else {
                            contentStarted = true
                            fullContent.append(delta.content)
                            onChunk(delta.content)
                        }
                    }
                }
            }
        }

        return fullContent.toString()
    }

    private fun extractOpenAiDelta(payload: String): OpenAiDelta {
        return try {
            val root = JSONObject(payload)
            val choice = root.optJSONArray("choices")?.optJSONObject(0) ?: return OpenAiDelta()
            val delta = choice.optJSONObject("delta") ?: return OpenAiDelta()
            OpenAiDelta(
                content = delta.optString("content", ""),
                reasoning = delta.optString("reasoning", ""),
            )
        } catch (_: JSONException) {
            OpenAiDelta()
        }
    }

    private fun runGroqCompound(
        apiKey: String,
        model: PresetModelDescriptor,
        prompt: String,
        inputText: String,
        searchLabel: String?,
        onChunk: (String) -> Unit,
    ): String {
        if (apiKey.isBlank()) throw IOException("NO_API_KEY:groq")

        val payload = JSONObject()
            .put("model", model.fullName)
            .put(
                "messages",
                JSONArray()
                    .put(
                        JSONObject()
                            .put("role", "system")
                            .put(
                                "content",
                                "IMPORTANT: Limit yourself to a maximum of 3 tool calls total. Make 1-2 focused searches, then answer. Do not visit websites unless absolutely necessary. Be efficient.",
                            ),
                    )
                    .put(
                        JSONObject()
                            .put("role", "user")
                            .put("content", "$prompt\n\n$inputText"),
                    ),
            )
            .put("temperature", 1)
            .put("max_tokens", 8192)
            .put("stream", false)
            .put(
                "compound_custom",
                JSONObject().put(
                    "tools",
                    JSONObject().put(
                        "enabled_tools",
                        JSONArray().put("web_search").put("visit_website"),
                    ),
                ),
            )

        val request = Request.Builder()
            .url(GROQ_ENDPOINT)
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(payload.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("Groq request failed with $code")
            }

            val body = response.body?.string().orEmpty()
            val content = try {
                val root = JSONObject(body)
                root.optJSONArray("choices")
                    ?.optJSONObject(0)
                    ?.optJSONObject("message")
                    ?.optString("content", "")
                    .orEmpty()
            } catch (_: JSONException) {
                ""
            }

            if (content.isBlank()) {
                throw IOException(
                    if (searchLabel.isNullOrBlank()) {
                        "Groq compound returned blank content."
                    } else {
                        "Groq compound returned blank content for $searchLabel."
                    },
                )
            }

            onChunk(content)
            return content
        }
    }

    private fun streamOllama(
        baseUrl: String,
        model: PresetModelDescriptor,
        prompt: String,
        inputText: String,
        uiLanguage: String,
        onChunk: (String) -> Unit,
    ): String {
        if (baseUrl.isBlank()) throw IOException("OLLAMA_URL_MISSING")

        val payload = JSONObject()
            .put("model", model.fullName)
            .put("prompt", "$prompt\n\n$inputText")
            .put("stream", true)

        val request = Request.Builder()
            .url("${baseUrl.trimEnd('/')}/api/generate")
            .header("Content-Type", "application/json")
            .post(payload.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()

        val fullContent = StringBuilder()
        var thinkingShown = false
        var contentStarted = false

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Ollama request failed with ${response.code}")
            }

            val body = response.body ?: throw IOException("Ollama response body was empty.")
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    val line = rawLine.trim()
                    if (line.isEmpty()) return@forEach

                    try {
                        val json = JSONObject(line)
                        val thinking = json.optString("thinking", "")
                        val responseText = json.optString("response", "")
                        if (thinking.isNotEmpty() && !thinkingShown && !contentStarted) {
                            onChunk(thinkingLabel(uiLanguage))
                            thinkingShown = true
                        }
                        if (responseText.isNotEmpty()) {
                            if (!contentStarted && thinkingShown) {
                                contentStarted = true
                                fullContent.append(responseText)
                                onChunk("$WIPE_SIGNAL$fullContent")
                            } else {
                                contentStarted = true
                                fullContent.append(responseText)
                                onChunk(responseText)
                            }
                        }
                        if (json.optBoolean("done", false)) {
                            return@useLines
                        }
                    } catch (_: JSONException) {
                        return@forEach
                    }
                }
            }
        }

        return fullContent.toString()
    }

    private fun translateGoogleGtx(
        inputText: String,
        prompt: String,
        onChunk: (String) -> Unit,
    ): String {
        val targetLanguage = prompt
            .lowercase()
            .substringAfter("translate to ", "")
            .substringBefore(".")
            .substringBefore(",")
            .trim()
            .replaceFirstChar { it.uppercaseChar() }
            .ifBlank { "English" }

        val targetCode = LanguageCatalog.codeForName(targetLanguage).lowercase()
        val encoded = URLEncoder.encode(inputText, "UTF-8")
        val request = Request.Builder()
            .url("$GTX_ENDPOINT?client=gtx&sl=auto&tl=$targetCode&dt=t&q=$encoded")
            .header("User-Agent", "Mozilla/5.0")
            .build()

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("GTX translation failed with ${response.code}")
            }
            val body = response.body?.string().orEmpty()
            val sentences = JSONArray(body).optJSONArray(0)
                ?: throw IOException("GTX returned no translation segments.")
            val result = buildString {
                for (index in 0 until sentences.length()) {
                    append(sentences.optJSONArray(index)?.optString(0).orEmpty())
                }
            }
            if (result.isBlank()) throw IOException("GTX returned blank translation.")
            onChunk(result)
            return result
        }
    }

    fun debugResolveTextRequest(modelId: String): ResolvedTextRequest {
        val model = resolveModel(modelId)
        return ResolvedTextRequest(
            modelId = model.id,
            provider = model.provider,
            apiModel = model.fullName,
            supportsSearch = PresetModelCatalog.supportsSearchByName(model.fullName),
            geminiThinkingConfig = PresetModelCatalog.geminiThinkingConfig(model.fullName),
        )
    }

    fun debugBuildRequestBody(
        modelId: String,
        prompt: String,
        inputText: String,
    ): String {
        val model = resolveModel(modelId)
        return when (model.provider) {
            PresetModelProvider.GOOGLE -> {
                buildGeminiDebugPayload(
                    fullName = model.fullName,
                    prompt = prompt,
                    inputText = inputText,
                )
            }
            PresetModelProvider.GROQ -> {
                if (model.fullName.startsWith("groq/compound")) {
                    buildGroqCompoundDebugPayload(
                        fullName = model.fullName,
                        prompt = prompt,
                        inputText = inputText,
                    )
                } else {
                    buildOpenAiCompatibleDebugPayload(
                        fullName = model.fullName,
                        prompt = prompt,
                        inputText = inputText,
                    )
                }
            }
            else -> buildOpenAiCompatibleDebugPayload(
                fullName = model.fullName,
                prompt = prompt,
                inputText = inputText,
            )
        }
    }

    private fun buildGeminiDebugPayload(
        fullName: String,
        prompt: String,
        inputText: String,
    ): String {
        val payload = buildJsonObject {
            putJsonArray("contents") {
                add(
                    buildJsonObject {
                        put("role", "user")
                        putJsonArray("parts") {
                            add(buildJsonObject { put("text", "$prompt\n\n$inputText") })
                        }
                    },
                )
            }
            PresetModelCatalog.geminiThinkingConfig(fullName)?.let { thinking ->
                putJsonObject("generationConfig") {
                    putJsonObject("thinkingConfig") {
                        thinking.forEach { (key, value) ->
                            when (value) {
                                is Boolean -> put(key, value)
                                is Number -> put(key, value.toDouble())
                                else -> put(key, value.toString())
                            }
                        }
                    }
                }
            }
            if (PresetModelCatalog.supportsSearchByName(fullName)) {
                putJsonArray("tools") {
                    add(buildJsonObject { putJsonObject("url_context") {} })
                    add(buildJsonObject { putJsonObject("google_search") {} })
                }
            }
        }
        return debugJson.encodeToString(kotlinx.serialization.json.JsonObject.serializer(), payload)
    }

    private fun buildGroqCompoundDebugPayload(
        fullName: String,
        prompt: String,
        inputText: String,
    ): String {
        val payload = buildJsonObject {
            put("model", fullName)
            putJsonArray("messages") {
                add(
                    buildJsonObject {
                        put("role", "system")
                        put(
                            "content",
                            "IMPORTANT: Limit yourself to a maximum of 3 tool calls total. Make 1-2 focused searches, then answer. Do not visit websites unless absolutely necessary. Be efficient.",
                        )
                    },
                )
                add(
                    buildJsonObject {
                        put("role", "user")
                        put("content", "$prompt\n\n$inputText")
                    },
                )
            }
            put("temperature", 1)
            put("max_tokens", 8192)
            put("stream", false)
            putJsonObject("compound_custom") {
                putJsonObject("tools") {
                    putJsonArray("enabled_tools") {
                        add(JsonPrimitive("web_search"))
                        add(JsonPrimitive("visit_website"))
                    }
                }
            }
        }
        return debugJson.encodeToString(kotlinx.serialization.json.JsonObject.serializer(), payload)
    }

    private fun buildOpenAiCompatibleDebugPayload(
        fullName: String,
        prompt: String,
        inputText: String,
    ): String {
        val payload = buildJsonObject {
            put("model", fullName)
            putJsonArray("messages") {
                add(
                    buildJsonObject {
                        put("role", "user")
                        put("content", "$prompt\n\n$inputText")
                    },
                )
            }
            put("stream", true)
        }
        return debugJson.encodeToString(kotlinx.serialization.json.JsonObject.serializer(), payload)
    }

    private fun openAiPayload(
        fullName: String,
        prompt: String,
        inputText: String,
    ): JSONObject {
        return JSONObject()
            .put("model", fullName)
            .put(
                "messages",
                JSONArray().put(
                    JSONObject()
                        .put("role", "user")
                        .put("content", "$prompt\n\n$inputText"),
                ),
            )
            .put("stream", true)
    }

    private fun thinkingLabel(uiLanguage: String): String = when (uiLanguage) {
        "vi" -> "AI đang suy nghĩ..."
        "ko" -> "AI가 생각하는 중..."
        else -> "AI is thinking..."
    }

    companion object {
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

        private const val GEMINI_ENDPOINT =
            "https://generativelanguage.googleapis.com/v1beta/models"
        private const val CEREBRAS_ENDPOINT =
            "https://api.cerebras.ai/v1/chat/completions"
        private const val GROQ_ENDPOINT =
            "https://api.groq.com/openai/v1/chat/completions"
        private const val OPENROUTER_ENDPOINT =
            "https://openrouter.ai/api/v1/chat/completions"
        private const val GTX_ENDPOINT =
            "https://translate.googleapis.com/translate_a/single"

        const val WIPE_SIGNAL: String = "\u0000WIPE\u0000"
        private val debugJson = Json { prettyPrint = false }
    }
}

data class ResolvedTextRequest(
    val modelId: String,
    val provider: PresetModelProvider,
    val apiModel: String,
    val supportsSearch: Boolean,
    val geminiThinkingConfig: Map<String, Any>?,
)

private data class GeminiDelta(
    val content: String = "",
    val reasoning: Boolean = false,
)

private data class OpenAiDelta(
    val content: String = "",
    val reasoning: String = "",
)
