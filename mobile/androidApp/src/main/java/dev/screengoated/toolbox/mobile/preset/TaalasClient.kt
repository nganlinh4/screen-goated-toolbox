package dev.screengoated.toolbox.mobile.preset

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException

/**
 * Taalas / chatjimmy.ai API client (Llama 3.1 8B on HC1 silicon, ~17,000 tok/s).
 *
 * Single shared entry point — change endpoint, model, or response parsing here only.
 */
object TaalasClient {
    private const val ENDPOINT = "https://chatjimmy.ai/api/chat"
    private const val MODEL = "llama3.1-8B"
    private const val TOP_K = 8
    private const val STATS_MARKER = "<|stats|>"
    private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

    /**
     * Send a prompt to chatjimmy.ai and return the clean response text.
     *
     * @return clean text with stats trailer stripped, or `null` on failure / blank response.
     */
    fun generate(httpClient: OkHttpClient, prompt: String): String? {
        val body = JSONObject()
            .put("messages", JSONArray().put(JSONObject().put("role", "user").put("content", prompt)))
            .put("chatOptions", JSONObject().put("selectedModel", MODEL).put("topK", TOP_K))
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)

        val request = Request.Builder()
            .url(ENDPOINT)
            .header("Content-Type", "application/json")
            .post(body)
            .build()

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) return null
            val raw = response.body?.string().orEmpty()
            val statsIdx = raw.indexOf(STATS_MARKER)
            val clean = if (statsIdx >= 0) raw.substring(0, statsIdx).trim() else raw.trim()
            return clean.ifBlank { null }
        }
    }

    /**
     * Convenience: generate or throw [IOException].
     */
    fun generateOrThrow(httpClient: OkHttpClient, prompt: String): String {
        return generate(httpClient, prompt)
            ?: throw IOException("Taalas API returned empty or failed response")
    }
}
