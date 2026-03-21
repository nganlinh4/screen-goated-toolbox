package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.ensureActive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import kotlin.coroutines.coroutineContext

internal suspend fun VisionApiClient.streamOpenAiVision(
    endpoint: String,
    apiKey: String,
    providerName: String,
    model: PresetModelDescriptor,
    prompt: String,
    imageBase64: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:${providerName.lowercase()}")

    val payload = openAiVisionPayload(model.fullName, prompt, imageBase64, streamingEnabled)

    if (!streamingEnabled) {
        return generateOpenAiVisionBlocking(endpoint, apiKey, providerName, model, payload, onChunk)
    }

    val request = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
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
            throw IOException("$providerName vision request failed with $code")
        }

        val rlRemaining = response.header("x-ratelimit-remaining-requests")
            ?: response.header("x-ratelimit-remaining-requests-day")
        val rlLimit = response.header("x-ratelimit-limit-requests")
            ?: response.header("x-ratelimit-limit-requests-day")
        ModelUsageStats.update(model.fullName, rlRemaining, rlLimit)

        val body = response.body ?: throw IOException("$providerName vision response body was empty.")
        body.charStream().buffered().useLines { lines ->
            lines.forEach { rawLine ->
                coroutineContext.ensureActive()
                val line = rawLine.trim()
                if (!line.startsWith("data: ")) return@forEach
                val data = line.removePrefix("data: ").trim()
                if (data.isBlank() || data == "[DONE]") return@forEach

                val delta = extractOpenAiDelta(data)
                if (delta.reasoning.isNotEmpty() && !thinkingShown && !contentStarted) {
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

private fun VisionApiClient.generateOpenAiVisionBlocking(
    endpoint: String,
    apiKey: String,
    providerName: String,
    model: PresetModelDescriptor,
    payload: JSONObject,
    onChunk: (String) -> Unit,
): String {
    val request = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    httpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
            throw IOException("$providerName vision request failed with $code")
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
            throw IOException("$providerName vision returned blank content.")
        }
        onChunk(content)
        return content
    }
}

internal fun VisionApiClient.callQrServer(
    imageBytes: ByteArray,
    onChunk: (String) -> Unit,
): String {
    val body = MultipartBody.Builder()
        .setType(MultipartBody.FORM)
        .addFormDataPart("MAX_FILE_SIZE", "1048576")
        .addFormDataPart(
            "file",
            "qrcode.png",
            imageBytes.toRequestBody("image/png".toMediaType()),
        )
        .build()

    val request = Request.Builder()
        .url("https://api.qrserver.com/v1/read-qr-code/")
        .post(body)
        .build()

    httpClient.newCall(request).execute().use { response ->
        if (!response.isSuccessful) {
            throw IOException("QR server request failed with ${response.code}")
        }
        val responseBody = response.body?.string().orEmpty()
        val data = try {
            val arr = JSONArray(responseBody)
            arr.optJSONObject(0)
                ?.optJSONArray("symbol")
                ?.optJSONObject(0)
                ?.optString("data", "")
                .orEmpty()
        } catch (_: JSONException) {
            ""
        }
        if (data.isBlank()) {
            throw IOException("QR code not detected in image.")
        }
        onChunk(data)
        return data
    }
}

private fun openAiVisionPayload(
    fullName: String,
    prompt: String,
    imageBase64: String,
    stream: Boolean,
): JSONObject {
    val content = JSONArray()
        .put(JSONObject().put("type", "text").put("text", prompt))
        .put(
            JSONObject()
                .put("type", "image_url")
                .put(
                    "image_url",
                    JSONObject().put("url", "data:image/png;base64,$imageBase64"),
                ),
        )

    return JSONObject()
        .put("model", fullName)
        .put(
            "messages",
            JSONArray().put(
                JSONObject()
                    .put("role", "user")
                    .put("content", content),
            ),
        )
        .put("stream", stream)
}
