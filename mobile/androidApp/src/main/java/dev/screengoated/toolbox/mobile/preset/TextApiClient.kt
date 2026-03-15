package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.net.URLEncoder
import kotlin.coroutines.coroutineContext

data class ApiKeys(
    val geminiKey: String = "",
    val cerebrasKey: String = "",
    val groqKey: String = "",
)

class TextApiClient(private val httpClient: OkHttpClient) {

    /**
     * Streams a text completion from the resolved provider, delivering incremental
     * chunks via [onChunk]. Returns the full accumulated response on success.
     */
    suspend fun executeStreaming(
        model: String,
        prompt: String,
        inputText: String,
        apiKeys: ApiKeys,
        onChunk: (String) -> Unit,
    ): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val provider = resolveProvider(model)
            when (provider) {
                PROVIDER_GEMINI -> streamGemini(model, prompt, inputText, apiKeys.geminiKey, onChunk)
                PROVIDER_CEREBRAS -> streamOpenAiCompatible(
                    endpoint = CEREBRAS_ENDPOINT,
                    apiKey = apiKeys.cerebrasKey,
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    providerName = "Cerebras",
                    onChunk = onChunk,
                )
                PROVIDER_GROQ -> streamOpenAiCompatible(
                    endpoint = GROQ_ENDPOINT,
                    apiKey = apiKeys.groqKey,
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    providerName = "Groq",
                    onChunk = onChunk,
                )
                PROVIDER_GTX -> translateGoogleGtx(inputText, prompt, onChunk)
                else -> throw IOException("Unknown provider: $provider")
            }
        }
    }

    // ---- Provider resolution ------------------------------------------------

    private fun resolveProvider(model: String): String {
        return when {
            model == "google-gtx" -> PROVIDER_GTX
            model.startsWith("cerebras_") -> PROVIDER_CEREBRAS
            model.startsWith("gemini-") ||
                model.startsWith("compound_") ||
                model.startsWith("text_gemini_") -> PROVIDER_GEMINI
            model.startsWith("groq_") ||
                model.startsWith("whisper-") -> PROVIDER_GROQ
            else -> PROVIDER_GEMINI // default fallback
        }
    }

    // ---- Gemini streaming ---------------------------------------------------

    private suspend fun streamGemini(
        model: String,
        prompt: String,
        inputText: String,
        apiKey: String,
        onChunk: (String) -> Unit,
    ): String {
        if (apiKey.isBlank()) throw IOException("NO_API_KEY:gemini")

        val combinedPrompt = "$prompt\n\n$inputText"
        val payload = JSONObject()
            .put(
                "contents",
                JSONArray().put(
                    JSONObject()
                        .put("role", "user")
                        .put("parts", JSONArray().put(JSONObject().put("text", combinedPrompt))),
                ),
            )

        val url = "$GEMINI_ENDPOINT/$model:streamGenerateContent?alt=sse&key=$apiKey"
        val requestBody = payload.toString().toRequestBody(JSON_MEDIA_TYPE)
        val httpRequest = Request.Builder()
            .url(url)
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        val fullContent = StringBuilder()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("Gemini request failed with $code")
            }
            val body = response.body ?: throw IOException("Gemini response body was empty.")
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    coroutineContext.ensureActive()
                    val line = rawLine.trim()
                    if (!line.startsWith("data: ")) return@forEach
                    val data = line.removePrefix("data: ").trim()
                    if (data.isBlank() || data == "[DONE]") return@forEach

                    val text = extractGeminiText(data)
                    if (text.isNotEmpty()) {
                        fullContent.append(text)
                        onChunk(text)
                    }
                }
            }
        }

        return fullContent.toString()
    }

    private fun extractGeminiText(payload: String): String {
        return try {
            val root = JSONObject(payload)
            val candidates = root.optJSONArray("candidates") ?: return ""
            val candidate = candidates.optJSONObject(0) ?: return ""
            val parts = candidate
                .optJSONObject("content")
                ?.optJSONArray("parts") ?: return ""

            buildString {
                for (i in 0 until parts.length()) {
                    val part = parts.optJSONObject(i) ?: continue
                    // Skip thinking parts
                    if (part.optBoolean("thought", false)) continue
                    val text = part.optString("text", "")
                    if (text.isNotEmpty()) append(text)
                }
            }
        } catch (_: JSONException) {
            ""
        }
    }

    // ---- OpenAI-compatible streaming (Cerebras / Groq) ----------------------

    private suspend fun streamOpenAiCompatible(
        endpoint: String,
        apiKey: String,
        model: String,
        prompt: String,
        inputText: String,
        providerName: String,
        onChunk: (String) -> Unit,
    ): String {
        if (apiKey.isBlank()) throw IOException("NO_API_KEY:${providerName.lowercase()}")

        val combinedPrompt = "$prompt\n\n$inputText"
        val messages = JSONArray()
            .put(JSONObject().put("role", "user").put("content", combinedPrompt))

        val payload = JSONObject()
            .put("model", model)
            .put("messages", messages)
            .put("stream", true)

        val requestBody = payload.toString().toRequestBody(JSON_MEDIA_TYPE)
        val httpRequest = Request.Builder()
            .url(endpoint)
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        val fullContent = StringBuilder()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("$providerName request failed with $code")
            }
            val body = response.body ?: throw IOException("$providerName response body was empty.")
            body.charStream().buffered().useLines { lines ->
                lines.forEach { rawLine ->
                    coroutineContext.ensureActive()
                    val line = rawLine.trim()
                    if (!line.startsWith("data: ")) return@forEach
                    val data = line.removePrefix("data: ").trim()
                    if (data.isBlank() || data == "[DONE]") return@forEach

                    val delta = extractSseDelta(data)
                    if (delta.isNotEmpty()) {
                        fullContent.append(delta)
                        onChunk(delta)
                    }
                }
            }
        }

        return fullContent.toString()
    }

    private fun extractSseDelta(payload: String): String {
        return try {
            val root = JSONObject(payload)
            val choices = root.optJSONArray("choices") ?: return ""
            val choice = choices.optJSONObject(0) ?: return ""
            choice.optJSONObject("delta")?.optString("content", "").orEmpty()
        } catch (_: JSONException) {
            ""
        }
    }

    // ---- Google Translate GTX -----------------------------------------------

    private fun translateGoogleGtx(
        inputText: String,
        prompt: String,
        onChunk: (String) -> Unit,
    ): String {
        // Extract target language from the prompt (e.g. "Translate to Korean")
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
        val url = "$GTX_ENDPOINT?client=gtx&sl=auto&tl=$targetCode&dt=t&q=$encoded"

        val httpRequest = Request.Builder()
            .url(url)
            .header("User-Agent", "Mozilla/5.0")
            .build()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("GTX translation failed with ${response.code}")
            }
            val body = response.body?.string().orEmpty()
            val sentences = JSONArray(body).optJSONArray(0)
                ?: throw IOException("GTX returned no translation segments.")
            val result = buildString {
                for (i in 0 until sentences.length()) {
                    append(sentences.optJSONArray(i)?.optString(0).orEmpty())
                }
            }
            if (result.isBlank()) throw IOException("GTX returned blank translation.")
            onChunk(result)
            return result
        }
    }

    private companion object {
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

        private const val PROVIDER_GEMINI = "gemini"
        private const val PROVIDER_CEREBRAS = "cerebras"
        private const val PROVIDER_GROQ = "groq"
        private const val PROVIDER_GTX = "google-gtx"

        private const val GEMINI_ENDPOINT =
            "https://generativelanguage.googleapis.com/v1beta/models"
        private const val CEREBRAS_ENDPOINT =
            "https://api.cerebras.ai/v1/chat/completions"
        private const val GROQ_ENDPOINT =
            "https://api.groq.com/openai/v1/chat/completions"
        private const val GTX_ENDPOINT =
            "https://translate.googleapis.com/translate_a/single"
    }
}
