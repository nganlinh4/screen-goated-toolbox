package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.ensureActive
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
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
    searchEnabled: Boolean = false,
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
            searchEnabled = searchEnabled,
        )
    }

    val payload = openAiPayload(
        model.provider, model.fullName, prompt, inputText,
        stream = true, searchEnabled = searchEnabled,
    )
    val request = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(
            payload.toString()
                .toRequestBody(jsonMediaType),
        )
        .build()

    val fullContent = StringBuilder()
    var thinkingShown = false
    var contentStarted = false
    httpClient.executePresetOpenAiRequest(request, payload, providerName, model, streamingEnabled = true).use { response ->
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException(invalidApiKeyMessage(providerName))
            throw IOException(response.providerFailureMessage("$providerName request"))
        }

        val body = response.body
        val context = coroutineContext
        body.charStream().buffered().use { reader ->
            consumeOpenAiCompletion(reader, { context.ensureActive() }) { delta ->
                if (delta.reasoning.isNotEmpty() && !thinkingShown && !contentStarted) {
                    onChunk(thinkingLabel(uiLanguage))
                    thinkingShown = true
                }
                if (delta.content.isNotEmpty()) {
                    response.firstOutputReceived()
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

    httpClient.newPresetCall(request, model, streamingEnabled = false).execute().use { response ->
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException(invalidApiKeyMessage("groq"))
            throw IOException(response.providerFailureMessage("Groq request"))
        }

        val content = parseOpenAiCompletion(response.body.string())

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
    searchEnabled: Boolean,
): String {
    val payload = openAiPayload(
        model.provider, model.fullName, prompt, inputText,
        stream = false, searchEnabled = searchEnabled,
    )
    val request = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(
            payload.toString()
                .toRequestBody(jsonMediaType),
        )
        .build()

    httpClient.executePresetOpenAiRequest(request, payload, providerName, model, streamingEnabled = false).use { response ->

        val content = parseOpenAiCompletion(response.body.string())
        if (content.isBlank()) {
            throw IOException("$providerName returned blank content.")
        }
        onChunk(content)
        return content
    }
}
