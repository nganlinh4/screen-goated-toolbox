package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import kotlin.coroutines.coroutineContext

internal suspend fun TextApiClient.streamGemini(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    apiKey: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
    searchEnabled: Boolean,
): String {
    return if (streamingEnabled) {
        streamGeminiStreaming(
            model = model,
            prompt = prompt,
            inputText = inputText,
            apiKey = apiKey,
            uiLanguage = uiLanguage,
            onChunk = onChunk,
            searchEnabled = searchEnabled,
        )
    } else {
        generateGeminiBlocking(
            model = model,
            prompt = prompt,
            inputText = inputText,
            apiKey = apiKey,
            searchEnabled = searchEnabled,
        )
    }
}

private suspend fun TextApiClient.streamGeminiStreaming(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    apiKey: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    searchEnabled: Boolean,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")

    val payload = buildGeminiPayload(model, prompt, inputText, searchEnabled)
    val request = Request.Builder()
        .url("$GEMINI_ENDPOINT/${model.fullName}:streamGenerateContent?alt=sse")
        .header("x-goog-api-key", apiKey)
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    val fullContent = StringBuilder()
    var thinkingShown = false
    var contentStarted = false

    val job = coroutineContext[Job]
    val result = httpClient.newPresetCall(request, model, streamingEnabled = true, job = job).execute().use { response ->
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException(invalidApiKeyMessage("google"))
            throw IOException(response.providerFailureMessage("Gemini request"))
        }

        val body = response.body
        body.charStream().buffered().useLines { lines ->
            consumeGeminiStream(lines.onEach { job?.ensureActive() }) { delta ->
                if (delta.reasoning && !thinkingShown && !contentStarted) {
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

    job?.ensureActive()
    return result
}

private suspend fun TextApiClient.generateGeminiBlocking(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    apiKey: String,
    searchEnabled: Boolean,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")
    val payload = buildGeminiPayload(model, prompt, inputText, searchEnabled)
    val request = Request.Builder()
        .url("$GEMINI_ENDPOINT/${model.fullName}:generateContent")
        .header("x-goog-api-key", apiKey)
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    httpClient.newPresetCall(request, model, streamingEnabled = false).execute().use { response ->
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException(invalidApiKeyMessage("google"))
            throw IOException(response.providerFailureMessage("Gemini request"))
        }

        val body = response.body
        val root = try { JSONObject(body.string()) } catch (error: org.json.JSONException) {
            throw IOException("PROVIDER_RESPONSE_INVALID:Malformed provider response", error)
        }
        return parseGeminiCompletion(root)
    }
}

internal fun buildGeminiPayload(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    searchEnabled: Boolean,
): JSONObject {
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
    PresetModelCatalog.geminiThinkingConfig(model.provider, model.fullName)?.let { thinking ->
        val thinkingConfig = JSONObject().apply {
            thinking.forEach { (key, value) ->
                when (value) {
                    is Boolean -> put(key, value)
                    is Number -> put(key, value)
                    else -> put(key, value.toString())
                }
            }
        }
        payload.put(
            "generationConfig",
            JSONObject().put("thinkingConfig", thinkingConfig),
        )
    }
    if (searchEnabled) {
        payload.put(
            "tools",
            JSONArray().put(JSONObject().put("google_search", JSONObject())),
        )
    }
    // Search support is catalog capability metadata. Ordinary generation must
    // not spend grounding quota unless a caller uses an explicit search path.
    return payload
}
