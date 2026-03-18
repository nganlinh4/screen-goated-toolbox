package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
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
): String {
    return if (streamingEnabled) {
        streamGeminiStreaming(
            model = model,
            prompt = prompt,
            inputText = inputText,
            apiKey = apiKey,
            uiLanguage = uiLanguage,
            onChunk = onChunk,
        )
    } else {
        generateGeminiBlocking(
            model = model,
            prompt = prompt,
            inputText = inputText,
            apiKey = apiKey,
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
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")

    val payload = buildGeminiPayload(model, prompt, inputText)
    val request = Request.Builder()
        .url("$GEMINI_ENDPOINT/${model.fullName}:streamGenerateContent?alt=sse")
        .header("x-goog-api-key", apiKey)
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
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

private suspend fun TextApiClient.generateGeminiBlocking(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    apiKey: String,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")
    val payload = buildGeminiPayload(model, prompt, inputText)
    val request = Request.Builder()
        .url("$GEMINI_ENDPOINT/${model.fullName}:generateContent")
        .header("x-goog-api-key", apiKey)
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    httpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
            throw IOException("Gemini request failed with $code")
        }

        val body = response.body ?: throw IOException("Gemini response body was empty.")
        val root = JSONObject(body.string())
        val parts = root.optJSONArray("candidates")
            ?.optJSONObject(0)
            ?.optJSONObject("content")
            ?.optJSONArray("parts") ?: return ""

        val result = StringBuilder()
        for (index in 0 until parts.length()) {
            val part = parts.optJSONObject(index) ?: continue
            if (part.optBoolean("thought", false)) continue
            result.append(part.optString("text", ""))
        }
        return result.toString()
    }
}

internal fun buildGeminiPayload(
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
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
    PresetModelCatalog.geminiThinkingConfig(model.fullName)?.let { thinking ->
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
    if (PresetModelCatalog.supportsSearchByName(model.fullName)) {
        payload.put(
            "tools",
            JSONArray()
                .put(JSONObject().put("url_context", JSONObject()))
                .put(JSONObject().put("google_search", JSONObject())),
        )
    }
    return payload
}

internal fun extractGeminiDelta(payload: String): GeminiDelta {
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
