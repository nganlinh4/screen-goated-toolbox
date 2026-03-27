package dev.screengoated.toolbox.mobile.helpassistant

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException

class HelpAssistantClient(
    private val httpClient: OkHttpClient,
) {
    suspend fun ask(request: HelpAssistantRequest): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val apiKey = request.geminiApiKey.trim()
            if (apiKey.isEmpty()) {
                throw IOException("Gemini API key not configured. Please set it in Global Settings.")
            }

            val contextXml = fetchContextXml(request.bucket)
            askGemini(
                apiKey = apiKey,
                mode = request.mode,
                question = request.question.trim(),
                contextXml = contextXml,
            )
        }
    }

    internal fun fetchContextXml(bucket: HelpAssistantBucket): String {
        val request = Request.Builder()
            .url(bucket.rawUrl())
            .build()

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Failed to fetch XML: HTTP ${response.code}")
            }
            val body = response.body?.string()
                ?: throw IOException("Failed to fetch XML: empty response body")
            return body
        }
    }

    internal fun buildGeminiPayload(
        mode: HelpAssistantMode,
        question: String,
        contextXml: String,
    ): JSONObject {
        val userMessage = buildUserMessage(
            mode = mode,
            question = question,
            contextXml = contextXml,
        )
        return JSONObject()
            .put(
                "contents",
                JSONArray().put(
                    JSONObject().put(
                        "parts",
                        JSONArray().put(
                            JSONObject().put("text", userMessage),
                        ),
                    ),
                ),
            )
            .put(
                "generationConfig",
                JSONObject()
                    .put("maxOutputTokens", mode.maxOutputTokens)
                    .put("temperature", 0.7),
            )
    }

    internal fun buildUserMessage(
        mode: HelpAssistantMode,
        question: String,
        contextXml: String,
    ): String = "$SYSTEM_PROMPT ${mode.promptInstruction}\n\n---\nSource Code Context:\n$contextXml\n---\n\nUser Question: $question"

    internal fun askGemini(
        apiKey: String,
        mode: HelpAssistantMode,
        question: String,
        contextXml: String,
    ): String {
        val payload = buildGeminiPayload(mode, question, contextXml)
        val request = Request.Builder()
            .url(geminiEndpoint(mode))
            .header("x-goog-api-key", apiKey)
            .header("Content-Type", "application/json")
            .post(payload.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("API request failed: HTTP ${response.code}")
            }

            val body = response.body?.string()
                ?: throw IOException("Failed to parse response: empty body")
            val json = JSONObject(body)
            val parts = json.optJSONArray("candidates")
                ?.optJSONObject(0)
                ?.optJSONObject("content")
                ?.optJSONArray("parts")
                ?: throw IOException("Failed to extract response text")

            val result = StringBuilder()
            for (index in 0 until parts.length()) {
                val part = parts.optJSONObject(index) ?: continue
                result.append(part.optString("text"))
            }
            val text = result.toString().trim()
            if (text.isEmpty()) {
                throw IOException("Failed to extract response text")
            }
            return text
        }
    }

    companion object {
        internal const val SYSTEM_PROMPT: String =
            "Answer the user in a helpful, concise and easy to understand way in the question's language, no made up infomation, only the true infomation. Go straight to the point, dont mention thing like \"Based on the source code\", if answer needs to mention the UI, be sure to use correct i18n locale terms matching the question's language. Format your response in Markdown."

        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

        internal fun geminiEndpoint(mode: HelpAssistantMode): String =
            "https://generativelanguage.googleapis.com/v1beta/models/${mode.modelId}:generateContent"
    }
}
