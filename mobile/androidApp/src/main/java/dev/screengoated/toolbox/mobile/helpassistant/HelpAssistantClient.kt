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
    private var cachedIndex: List<ChunkEntry>? = null

    suspend fun ask(request: HelpAssistantRequest): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val apiKey = request.geminiApiKey.trim()
            if (apiKey.isEmpty()) {
                throw IOException("Gemini API key not configured. Please set it in Global Settings.")
            }

            val index = fetchHelpIndex()
            val topChunks = rankHelpAssistantChunks(index, request.question.trim())
            val context = topChunks.joinToString("\n\n") { "=== ${it.path} ===\n${it.text}" }

            // Try primary model, fall back on error
            try {
                askGemini(apiKey, PRIMARY_MODEL, request.question.trim(), context)
            } catch (_: Exception) {
                askGemini(apiKey, FALLBACK_MODEL, request.question.trim(), context)
            }
        }
    }

    private fun fetchHelpIndex(): List<ChunkEntry> {
        cachedIndex?.let { return it }

        val request = Request.Builder().url(HELP_INDEX_URL).build()
        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Failed to fetch help index: HTTP ${response.code}")
            }
            val body = response.body?.string()
                ?: throw IOException("Failed to fetch help index: empty body")
            val arr = JSONArray(body)
            val entries = mutableListOf<ChunkEntry>()
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                entries.add(ChunkEntry(
                    path = obj.optString("path", ""),
                    text = obj.optString("text", ""),
                ))
            }
            cachedIndex = entries
            return entries
        }
    }

    private fun askGemini(
        apiKey: String,
        modelId: String,
        question: String,
        context: String,
    ): String {
        val userMessage = "$SYSTEM_PROMPT\n\n---\nSource Code Context:\n$context\n---\n\nUser Question: $question"
        val payload = JSONObject()
            .put(
                "contents",
                JSONArray().put(
                    JSONObject().put(
                        "parts",
                        JSONArray().put(JSONObject().put("text", userMessage)),
                    ),
                ),
            )
            .put(
                "generationConfig",
                JSONObject()
                    .put("maxOutputTokens", MAX_OUTPUT_TOKENS)
                    .put("temperature", 0.7)
                    .put("thinkingConfig", JSONObject().put("thinkingLevel", "MINIMAL")),
            )

        val url = "https://generativelanguage.googleapis.com/v1beta/models/$modelId:generateContent"
        val request = Request.Builder()
            .url(url)
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
            for (i in 0 until parts.length()) {
                val part = parts.optJSONObject(i) ?: continue
                // Skip thought parts (thinking model output)
                if (part.optBoolean("thought", false)) continue
                result.append(part.optString("text", ""))
            }
            val text = result.toString().trim()
            if (text.isEmpty()) throw IOException("Failed to extract response text")
            return text
        }
    }

    companion object {
        internal const val SYSTEM_PROMPT: String =
            "You are the SGT (Screen Goated Toolbox) Android app help assistant. The user is asking from the Android version of the app — assume questions are about the Android app unless they explicitly mention Windows. " +
            "Answer in a helpful, concise and easy to understand way in the question's language, no made up information, only true information. Go straight to the point. " +
            "Do not mention \"Based on the source code\". If the answer needs to mention UI elements, use correct i18n locale terms matching the question's language. Format your response in Markdown."

        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
    }
}

internal const val HELP_ASSISTANT_TOP_K = 20

internal data class ChunkEntry(val path: String, val text: String)

internal fun rankHelpAssistantChunks(index: List<ChunkEntry>, question: String): List<ChunkEntry> {
    val terms = question.lowercase()
        .split(Regex("[^a-zA-Z0-9_]+"))
        .filter { it.length >= 2 }

    if (terms.isEmpty()) return index.take(HELP_ASSISTANT_TOP_K)

    return index.mapIndexed { indexInSource, chunk ->
        val haystack = "${chunk.path}\n${chunk.text}".lowercase()
        val pathLower = chunk.path.lowercase()
        var score = 0.0
        for (term in terms) {
            val count = haystack.split(term).size - 1
            if (count > 0) score += 1.0 + kotlin.math.ln(count.toDouble())
            if (pathLower.contains(term)) score += 3.0
        }
        IndexedHelpChunk(chunk, score, indexInSource)
    }
        .filter { it.score > 0.0 }
        .sortedWith(compareByDescending<IndexedHelpChunk> { it.score }.thenBy { it.index })
        .take(HELP_ASSISTANT_TOP_K)
        .map { it.chunk }
}

private data class IndexedHelpChunk(
    val chunk: ChunkEntry,
    val score: Double,
    val index: Int,
)
