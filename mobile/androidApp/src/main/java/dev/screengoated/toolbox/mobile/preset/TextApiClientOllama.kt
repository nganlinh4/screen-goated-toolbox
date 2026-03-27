package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.net.URLEncoder

internal fun TextApiClient.streamOllama(
    baseUrl: String,
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
): String {
    if (baseUrl.isBlank()) throw IOException("OLLAMA_URL_MISSING")
    if (!streamingEnabled) {
        return generateOllamaBlocking(
            baseUrl = baseUrl,
            model = model,
            prompt = prompt,
            inputText = inputText,
            onChunk = onChunk,
        )
    }

    val payload = JSONObject()
        .put("model", model.fullName)
        .put("prompt", "$prompt\n\n$inputText")
        .put("stream", true)

    val request = Request.Builder()
        .url("${baseUrl.trimEnd('/')}/api/generate")
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
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

internal fun TextApiClient.translateGoogleGtx(
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

internal fun TextApiClient.translateTaalas(
    inputText: String,
    prompt: String,
    onChunk: (String) -> Unit,
): String {
    val result = TaalasClient.generateOrThrow(httpClient, "$prompt\n\n$inputText")
    onChunk(result)
    return result
}

private fun TextApiClient.generateOllamaBlocking(
    baseUrl: String,
    model: PresetModelDescriptor,
    prompt: String,
    inputText: String,
    onChunk: (String) -> Unit,
): String {
    val payload = JSONObject()
        .put("model", model.fullName)
        .put("prompt", "$prompt\n\n$inputText")
        .put("stream", false)

    val request = Request.Builder()
        .url("${baseUrl.trimEnd('/')}/api/generate")
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    httpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            throw IOException("Ollama request failed with ${response.code}")
        }

        val content = try {
            JSONObject(response.body?.string().orEmpty()).optString("response", "")
        } catch (_: JSONException) {
            ""
        }
        if (content.isBlank()) {
            throw IOException("Ollama returned blank content.")
        }
        onChunk(content)
        return content
    }
}
