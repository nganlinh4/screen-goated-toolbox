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

private const val GROQ_MAX_RATE_LIMIT_WAIT_SECONDS = 2L

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

    val plainTextEnvelope = needsPlainTextEnvelope(model, streamingEnabled)
    val payload = openAiVisionPayload(
        model,
        prompt,
        imageBase64,
        mimeType,
        streamingEnabled,
        plainTextEnvelope,
    )

    if (!streamingEnabled) {
        return generateOpenAiVisionBlocking(
            endpoint,
            apiKey,
            providerName,
            model,
            payload,
            plainTextEnvelope,
            onChunk,
        )
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

    executeOpenAiVisionRequest(request, providerName, model).use { response ->
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
    plainTextEnvelope: Boolean,
    onChunk: (String) -> Unit,
): String {
    val request = Request.Builder()
        .url(endpoint)
        .header("Authorization", "Bearer $apiKey")
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    executeOpenAiVisionRequest(request, providerName, model).use { response ->
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
        if (content.isBlank()) throw IOException("$providerName vision returned blank content.")
        val text = if (plainTextEnvelope) unwrapPlainTextEnvelope(content) else content
        onChunk(text)
        return text
    }
}

private suspend fun VisionApiClient.executeOpenAiVisionRequest(
    request: Request,
    providerName: String,
    model: PresetModelDescriptor,
): okhttp3.Response {
    var retried = false
    while (true) {
        coroutineContext.ensureActive()
        val response = httpClient.newCall(request).execute()
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
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

/**
 * Instruction appended when plain text has to travel inside a JSON envelope.
 */
internal const val PLAIN_TEXT_ENVELOPE_PROMPT: String =
    "\n\nRespond with a single JSON object of the form {\"text\": \"<all extracted text>\"} " +
        "and nothing else."

/**
 * Whether a plain-text extraction must be wrapped in a JSON envelope.
 *
 * Qwen 3.6 on Groq deterministically appends a re-tokenized repetition of the text
 * it just emitted when asked for bare text; constraining the reply to a JSON object
 * terminates generation cleanly. Mirrors the Windows vision request policy.
 */
internal fun needsPlainTextEnvelope(
    model: PresetModelDescriptor,
    streaming: Boolean,
): Boolean =
    !streaming &&
        model.structuredOutputPolicy == PresetStructuredOutputPolicy.JSON_OBJECT

/**
 * Recovers the text from a [PLAIN_TEXT_ENVELOPE_PROMPT] reply, failing open so a
 * malformed envelope degrades to the raw body instead of losing text.
 */
internal fun unwrapPlainTextEnvelope(content: String): String =
    try {
        JSONObject(content.trim()).takeIf { it.has("text") }?.optString("text").orEmpty()
            .ifEmpty { content }
    } catch (_: JSONException) {
        content
    }

internal fun openAiVisionPayload(
    model: PresetModelDescriptor,
    prompt: String,
    imageBase64: String,
    mimeType: String,
    stream: Boolean,
    plainTextEnvelope: Boolean = false,
): JSONObject {
    val provider = model.provider
    val fullName = model.fullName
    val effectivePrompt = if (plainTextEnvelope) prompt + PLAIN_TEXT_ENVELOPE_PROMPT else prompt
    val textPart = JSONObject().put("type", "text").put("text", effectivePrompt)
    val imagePart = JSONObject()
        .put("type", "image_url")
        .put(
            "image_url",
            JSONObject().put("url", "data:$mimeType;base64,$imageBase64"),
        )
    val content = JSONArray()
    when (model.visionInputOrder) {
        PresetVisionInputOrder.TEXT_FIRST -> content.put(textPart).put(imagePart)
        PresetVisionInputOrder.IMAGE_FIRST -> content.put(imagePart).put(textPart)
    }

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
    if (model.visionSamplingPolicy == PresetVisionSamplingPolicy.QWEN3_GROQ_NON_THINKING) {
        payload
            .put("reasoning_format", "hidden")
            .put("temperature", 0.7)
            .put("top_p", 0.8)
            .put("presence_penalty", 1.5)
    }
    model.visionMaxOutputTokens?.let { payload.put("max_completion_tokens", it) }
    if (plainTextEnvelope) {
        payload.put("response_format", JSONObject().put("type", "json_object"))
    }
    return applyFastReasoningPolicy(payload, provider, fullName)
}
