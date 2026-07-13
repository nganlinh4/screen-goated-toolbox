package dev.screengoated.toolbox.mobile.preset

import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.util.zip.GZIPOutputStream

internal val jsonMediaType = "application/json; charset=utf-8".toMediaType()
internal const val GEMINI_ENDPOINT = "https://generativelanguage.googleapis.com/v1beta/models"
internal const val CEREBRAS_ENDPOINT = "https://api.cerebras.ai/v1/chat/completions"
internal const val GROQ_ENDPOINT = "https://api.groq.com/openai/v1/chat/completions"
internal const val OPENROUTER_ENDPOINT = "https://openrouter.ai/api/v1/chat/completions"
internal const val GTX_ENDPOINT = "https://translate.googleapis.com/translate_a/single"
internal val debugJson = Json { prettyPrint = false }

internal data class GeminiDelta(
    val content: String = "",
    val reasoning: Boolean = false,
)

internal data class OpenAiDelta(
    val content: String = "",
    val reasoning: String = "",
)

internal data class EncodedJsonRequest(
    val body: RequestBody,
    val gzipEncoded: Boolean,
)

internal fun encodeCerebrasJson(payload: JSONObject): EncodedJsonRequest {
    val bytes = payload.toString().encodeToByteArray()
    if (bytes.size < 12 * 1024) {
        return EncodedJsonRequest(bytes.toRequestBody(jsonMediaType), false)
    }
    val compressed = ByteArrayOutputStream().use { output ->
        GZIPOutputStream(output).use { it.write(bytes) }
        output.toByteArray()
    }
    return EncodedJsonRequest(compressed.toRequestBody(jsonMediaType), true)
}

internal fun openAiPayload(
    fullName: String,
    prompt: String,
    inputText: String,
    stream: Boolean = true,
): JSONObject {
    return JSONObject()
        .put("model", fullName)
        .put(
            "messages",
            JSONArray().put(
                JSONObject()
                    .put("role", "user")
                    .put("content", "$prompt\n\n$inputText"),
            ),
        )
        .put("stream", stream)
}

internal fun cerebrasPayload(
    fullName: String,
    prompt: String,
    inputText: String,
    stream: Boolean = true,
    predictionContent: String? = null,
): JSONObject {
    val payload = JSONObject()
        .put("model", fullName)
        .put(
            "messages",
            JSONArray()
                .put(JSONObject().put("role", "system").put("content", prompt))
                .put(JSONObject().put("role", "user").put("content", inputText)),
        )
        .put("stream", stream)
        .put("max_completion_tokens", 8192)
    if (!predictionContent.isNullOrEmpty() &&
        (fullName == "gpt-oss-120b" || fullName == "zai-glm-4.7")
    ) {
        payload.put(
            "prediction",
            JSONObject().put("type", "content").put("content", predictionContent),
        )
    }
    return payload
}
