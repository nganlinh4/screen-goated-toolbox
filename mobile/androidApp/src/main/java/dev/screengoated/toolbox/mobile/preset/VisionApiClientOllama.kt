package dev.screengoated.toolbox.mobile.preset

import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException

internal fun VisionApiClient.streamOllamaVision(
    baseUrl: String,
    model: PresetModelDescriptor,
    prompt: String,
    imageBase64: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
): String {
    if (baseUrl.isBlank()) throw IOException("OLLAMA_URL_MISSING")

    val payload = JSONObject()
        .put("model", model.fullName)
        .put("prompt", prompt)
        .put("images", JSONArray().put(imageBase64))
        .put("stream", streamingEnabled)

    val request = Request.Builder()
        .url("${baseUrl.trimEnd('/')}/api/generate")
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    if (!streamingEnabled) {
        return generateOllamaVisionBlocking(request, onChunk)
    }

    val fullContent = StringBuilder()
    var thinkingShown = false
    var contentStarted = false

    httpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            throw IOException("Ollama vision request failed with ${response.code}")
        }

        val body = response.body ?: throw IOException("Ollama vision response body was empty.")
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
                            onChunk("${TextApiClient.WIPE_SIGNAL}$fullContent")
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

private fun VisionApiClient.generateOllamaVisionBlocking(
    request: Request,
    onChunk: (String) -> Unit,
): String {
    httpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            throw IOException("Ollama vision request failed with ${response.code}")
        }

        val content = try {
            JSONObject(response.body?.string().orEmpty()).optString("response", "")
        } catch (_: JSONException) {
            ""
        }
        if (content.isBlank()) {
            throw IOException("Ollama vision returned blank content.")
        }
        onChunk(content)
        return content
    }
}
