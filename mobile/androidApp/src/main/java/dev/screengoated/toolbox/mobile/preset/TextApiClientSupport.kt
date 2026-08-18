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

internal fun openAiPayload(
    provider: PresetModelProvider,
    fullName: String,
    prompt: String,
    inputText: String,
    stream: Boolean = true,
): JSONObject {
    val payload = JSONObject()
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
    return applyFastReasoningPolicy(payload, provider, fullName)
}


internal fun applyFastReasoningPolicy(
    payload: JSONObject,
    provider: PresetModelProvider,
    fullName: String,
): JSONObject {
    val effort = PresetModelCatalog.openAiReasoningEffort(provider, fullName)
    if (effort != null) {
        if (provider == PresetModelProvider.OPENROUTER) {
            payload.put("reasoning", JSONObject().put("effort", effort))
        } else {
            payload.put("reasoning_effort", effort)
        }
    }
    return payload
}
