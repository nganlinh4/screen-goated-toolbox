package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import kotlin.coroutines.coroutineContext

internal suspend fun VisionApiClient.streamGeminiVision(
    model: PresetModelDescriptor,
    prompt: String,
    imageBase64: String,
    mimeType: String,
    apiKey: String,
    uiLanguage: String,
    onChunk: (String) -> Unit,
    streamingEnabled: Boolean,
    responseSchema: JSONObject?,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")

    val payload = buildGeminiVisionPayload(model, prompt, imageBase64, mimeType, responseSchema)
    val action = if (streamingEnabled) "streamGenerateContent?alt=sse" else "generateContent"
    val request = Request.Builder()
        .url("$GEMINI_ENDPOINT/${model.fullName}:$action")
        .header("x-goog-api-key", apiKey)
        .header("Content-Type", "application/json")
        .post(payload.toString().toRequestBody(jsonMediaType))
        .build()

    if (!streamingEnabled) {
        return generateGeminiVisionBlocking(request, model)
    }

    val fullContent = StringBuilder()
    var thinkingShown = false
    var contentStarted = false

    val job = coroutineContext[Job]
    val result = httpClient.newPresetCall(request, model, streamingEnabled = true, job = job).execute().use { response ->
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException(invalidApiKeyMessage("google"))
            throw IOException(response.providerFailureMessage("Gemini vision request"))
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

private fun VisionApiClient.generateGeminiVisionBlocking(
    request: Request,
    model: PresetModelDescriptor,
): String {
    httpClient.newPresetCall(request, model, streamingEnabled = false).execute().use { response ->
        ModelUsageStats.update(model.provider, model.fullName, response.headers)
        if (!response.isSuccessful) {
            val code = response.code
            if (code == 401 || code == 403) throw IOException(invalidApiKeyMessage("google"))
            throw IOException(response.providerFailureMessage("Gemini vision request"))
        }

        val body = response.body
        val root = try { JSONObject(body.string()) } catch (error: org.json.JSONException) {
            throw IOException("PROVIDER_RESPONSE_INVALID:Malformed provider response", error)
        }
        return parseGeminiCompletion(root)
    }
}

internal fun buildGeminiVisionPayload(
    model: PresetModelDescriptor,
    prompt: String,
    imageBase64: String,
    mimeType: String,
    responseSchema: JSONObject? = null,
): JSONObject {
    val textPart = JSONObject().put("text", prompt)
    val imagePart = JSONObject().put(
        "inline_data",
        JSONObject()
            .put("mime_type", mimeType)
            .put("data", imageBase64),
    )
    val parts = JSONArray()
    when (model.visionInputOrder) {
        PresetVisionInputOrder.TEXT_FIRST -> parts.put(textPart).put(imagePart)
        PresetVisionInputOrder.IMAGE_FIRST -> parts.put(imagePart).put(textPart)
    }

    val payload = JSONObject().put(
        "contents",
        JSONArray().put(
            JSONObject()
                .put("role", "user")
                .put("parts", parts),
        ),
    )

    val generationConfig = JSONObject()
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
        generationConfig.put("thinkingConfig", thinkingConfig)
    }
    when (model.visionMediaResolution) {
        PresetVisionMediaResolution.PROVIDER_DEFAULT -> Unit
    }
    if (responseSchema != null &&
        model.structuredOutputPolicy == PresetStructuredOutputPolicy.STRICT_JSON_SCHEMA
    ) {
        generationConfig
            .put("responseMimeType", "application/json")
            .put("responseJsonSchema", responseSchema)
    }
    if (generationConfig.length() > 0) {
        payload.put("generationConfig", generationConfig)
    }

    return payload
}
