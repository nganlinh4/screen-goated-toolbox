package dev.screengoated.toolbox.mobile.preset

import android.util.Log
import kotlinx.coroutines.ensureActive
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import kotlin.coroutines.coroutineContext

internal suspend fun TextApiClient.streamOpenAiCompatible(
    endpoint: String,
    apiKey: String,
    providerName: String,
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:${providerName.lowercase()}")
    if (!streamingEnabled) {
        return generateOpenAiCompatibleBlocking(
            endpoint = endpoint,
            apiKey = apiKey,
            providerName = providerName,
            model = model,
            prompt = prompt,
            inputText = inputText,
            onChunk = onChunk,
        )
    }

    val request = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(openAiPayload(model.fullName, prompt, inputText).toString().toRequestBody(jsonMediaType))
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

        // Capture rate limit headers
        val rlRemaining = response.header("x-ratelimit-remaining-requests")
            ?: response.header("x-ratelimit-remaining-requests-day")
        val rlLimit = response.header("x-ratelimit-limit-requests")
            ?: response.header("x-ratelimit-limit-requests-day")
        ModelUsageStats.update(model.fullName, rlRemaining, rlLimit)

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
                        onChunk("${TextApiClient.WIPE_SIGNAL}$fullContent")
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

internal suspend fun TextApiClient.streamCerebras(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    apiKey: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:cerebras")
    if (!streamingEnabled) {
        return generateCerebrasBlocking(
            model = model,
            prompt = prompt,
            inputText = inputText,
            apiKey = apiKey,
            onChunk = onChunk,
        )
    }

    val request = Request.Builder()
        .url(CEREBRAS_ENDPOINT)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(openAiPayload(model.fullName, prompt, inputText).toString().toRequestBody(jsonMediaType))
        .build()

    val fullContent = StringBuilder()
    var thinkingShown = false
    var contentStarted = false
    val reasoningFallback = model.fullName.contains("gpt-oss") || model.fullName.contains("zai-glm")

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
                if ((delta.reasoning.isNotEmpty() || reasoningFallback) && !thinkingShown && !contentStarted) {
                    onChunk(thinkingLabel(uiLanguage))
                    thinkingShown = true
                }
                if (delta.content.isNotEmpty()) {
                    if (!contentStarted && thinkingShown) {
                        contentStarted = true
                        fullContent.append(delta.content)
                        onChunk("${TextApiClient.WIPE_SIGNAL}$fullContent")
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

internal fun extractOpenAiDelta(payload: String): OpenAiDelta {
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

internal fun TextApiClient.runGroqCompound(
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
        .post(payload.toString().toRequestBody(jsonMediaType))
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

private suspend fun TextApiClient.generateOpenAiCompatibleBlocking(
    endpoint: String,
    apiKey: String,
    providerName: String,
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    onChunk: (String) -> Unit,
): String {
    val request = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(openAiPayload(model.fullName, prompt, inputText, stream = false).toString().toRequestBody(jsonMediaType))
        .build()

    httpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
            throw IOException("$providerName request failed with $code")
        }

        val rlRemaining = response.header("x-ratelimit-remaining-requests")
            ?: response.header("x-ratelimit-remaining-requests-day")
        val rlLimit = response.header("x-ratelimit-limit-requests")
            ?: response.header("x-ratelimit-limit-requests-day")
        ModelUsageStats.update(model.fullName, rlRemaining, rlLimit)

        val content = try {
            JSONObject(response.body?.string().orEmpty())
                .optJSONArray("choices")
                ?.optJSONObject(0)
                ?.optJSONObject("message")
                ?.optString("content", "")
                .orEmpty()
        } catch (_: JSONException) {
            ""
        }
        if (content.isBlank()) {
            throw IOException("$providerName returned blank content.")
        }
        onChunk(content)
        return content
    }
}

private suspend fun TextApiClient.generateCerebrasBlocking(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    apiKey: String,
    onChunk: (String) -> Unit,
): String {
    val request = Request.Builder()
        .url(CEREBRAS_ENDPOINT)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(openAiPayload(model.fullName, prompt, inputText, stream = false).toString().toRequestBody(jsonMediaType))
        .build()

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

        val content = try {
            JSONObject(response.body?.string().orEmpty())
                .optJSONArray("choices")
                ?.optJSONObject(0)
                ?.optJSONObject("message")
                ?.optString("content", "")
                .orEmpty()
        } catch (_: JSONException) {
            ""
        }
        if (content.isBlank()) {
            throw IOException("Cerebras returned blank content.")
        }
        onChunk(content)
        return content
    }
}
