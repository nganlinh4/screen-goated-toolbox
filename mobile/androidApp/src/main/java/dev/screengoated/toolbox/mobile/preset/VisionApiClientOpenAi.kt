package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import kotlin.math.ceil
import kotlin.coroutines.coroutineContext

private const val QWEN_VISION_MAX_COMPLETION_TOKENS = 1_024
private const val GROQ_MAX_RATE_LIMIT_WAIT_SECONDS = 30L

internal suspend fun VisionApiClient.streamOpenAiVision(
    endpoint: String,
    apiKey: String,
    providerName: String,
    model: PresetModelDescriptor,
    prompt: String,
    imageBase64: String,
    mimeType: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:${providerName.lowercase()}")

    val payload = openAiVisionPayload(
        model.fullName,
        prompt,
        imageBase64,
        mimeType,
        streamingEnabled,
    )

    if (!streamingEnabled) {
        return generateOpenAiVisionBlocking(endpoint, apiKey, providerName, model, payload, onChunk)
    }

    val encoded = if (providerName == "Cerebras") encodeCerebrasJson(payload) else null
    val requestBuilder = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(encoded?.body ?: payload.toString().toRequestBody(jsonMediaType))
    if (encoded?.gzipEncoded == true) requestBuilder.header("Content-Encoding", "gzip")
    val request = requestBuilder.build()

    val fullContent = StringBuilder()
    var thinkingShown = false
    var contentStarted = false

    executeOpenAiVisionRequest(request, providerName).use { response ->

        if (providerName == "Cerebras") {
            ModelUsageStats.updateCerebras(model.fullName, response.headers)
        } else {
            val rlRemaining = response.header("x-ratelimit-remaining-requests")
            val rlLimit = response.header("x-ratelimit-limit-requests")
            ModelUsageStats.update(model.fullName, rlRemaining, rlLimit)
        }

        val body = response.body
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

private suspend fun VisionApiClient.generateOpenAiVisionBlocking(
    endpoint: String,
    apiKey: String,
    providerName: String,
    model: PresetModelDescriptor,
    payload: JSONObject,
    onChunk: (String) -> Unit,
): String {
    val encoded = if (providerName == "Cerebras") encodeCerebrasJson(payload) else null
    val requestBuilder = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(encoded?.body ?: payload.toString().toRequestBody(jsonMediaType))
    if (encoded?.gzipEncoded == true) requestBuilder.header("Content-Encoding", "gzip")
    val request = requestBuilder.build()

    executeOpenAiVisionRequest(request, providerName).use { response ->

        if (providerName == "Cerebras") {
            ModelUsageStats.updateCerebras(model.fullName, response.headers)
        } else {
            val rlRemaining = response.header("x-ratelimit-remaining-requests")
            val rlLimit = response.header("x-ratelimit-limit-requests")
            ModelUsageStats.update(model.fullName, rlRemaining, rlLimit)
        }

        val content = try {
            JSONObject(response.body.string().orEmpty())
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

private suspend fun VisionApiClient.executeOpenAiVisionRequest(
    request: Request,
    providerName: String,
): okhttp3.Response {
    var retried = false
    while (true) {
        coroutineContext.ensureActive()
        val response = httpClient.newCall(request).execute()
        if (response.isSuccessful) return response

        val code = response.code
        val retryAfterSeconds = response.header("retry-after")
            ?.toDoubleOrNull()
            ?.let(::ceil)
            ?.toLong()
        val body = response.body.string()
        response.close()
        val retryDelayMillis = groqVisionRetryDelayMillis(
            providerName = providerName,
            statusCode = code,
            alreadyRetried = retried,
            retryAfterSeconds = retryAfterSeconds,
        )
        if (retryDelayMillis != null) {
            retried = true
            delay(retryDelayMillis)
            continue
        }
        if (code == 401 || code == 403) {
            throw IOException(invalidApiKeyMessage(providerName))
        }
        throw IOException(
            "$providerName vision request failed with $code: ${providerErrorMessage(code, body)}",
        )
    }
}

internal fun groqVisionRetryDelayMillis(
    providerName: String,
    statusCode: Int,
    alreadyRetried: Boolean,
    retryAfterSeconds: Long?,
): Long? = retryAfterSeconds
    ?.takeIf {
        providerName == "Groq" &&
            statusCode == 429 &&
            !alreadyRetried &&
            it <= GROQ_MAX_RATE_LIMIT_WAIT_SECONDS
    }
    ?.times(1_000L)

private fun providerErrorMessage(code: Int, body: String): String = try {
    JSONObject(body).optJSONObject("error")?.optString("message")
        ?.takeIf(String::isNotBlank)
        ?: "HTTP $code"
} catch (_: JSONException) {
    "HTTP $code"
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
        val responseBody = response.body.string().orEmpty()
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

internal fun openAiVisionPayload(
    fullName: String,
    prompt: String,
    imageBase64: String,
    mimeType: String,
    stream: Boolean,
): JSONObject {
    val content = JSONArray()
        .put(JSONObject().put("type", "text").put("text", prompt))
        .put(
            JSONObject()
                .put("type", "image_url")
                .put(
                    "image_url",
                    JSONObject().put("url", "data:$mimeType;base64,$imageBase64"),
                ),
        )

    val payload = JSONObject()
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
    if (fullName.startsWith("qwen/")) {
        payload.put("reasoning_format", "hidden")
        payload.put("max_completion_tokens", QWEN_VISION_MAX_COMPLETION_TOKENS)
    }
    if (fullName == "gemma-4-31b") {
        payload.put("max_completion_tokens", 8192)
    }
    return payload
}
