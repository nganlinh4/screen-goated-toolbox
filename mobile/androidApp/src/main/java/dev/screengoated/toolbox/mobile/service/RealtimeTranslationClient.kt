package dev.screengoated.toolbox.mobile.service

import android.util.Log
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.preset.ApiKeys
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.preset.providerIsAvailable
import dev.screengoated.toolbox.mobile.preset.streamGeminiLiveText
import dev.screengoated.toolbox.mobile.shared.live.LiveTranslationModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.TranslationRequest
import dev.screengoated.toolbox.mobile.shared.live.TranslationResponse
import dev.screengoated.toolbox.mobile.shared.live.TranslationPatch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit

class RealtimeTranslationClient(
    private val httpClient: OkHttpClient,
) {
    suspend fun translate(
        geminiApiKey: String,
        cerebrasApiKey: String,
        groqApiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
        providerId: String,
        llmChain: List<String>,
        runtimeSettings: PresetRuntimeSettings,
    ): TranslationExecutionResult = withContext(Dispatchers.IO) {
        val primaryId = providerId
        runCatching {
            return@withContext translateWithExactProvider(
                geminiApiKey = geminiApiKey,
                cerebrasApiKey = cerebrasApiKey,
                groqApiKey = groqApiKey,
                request = request,
                targetLanguage = targetLanguage,
                providerId = primaryId,
                llmChain = llmChain,
                runtimeSettings = runtimeSettings,
            )
        }

        val fallbackId = if (primaryId == PROVIDER_GTX) PROVIDER_LLM else PROVIDER_GTX
        translateWithExactProvider(
            geminiApiKey = geminiApiKey,
            cerebrasApiKey = cerebrasApiKey,
            groqApiKey = groqApiKey,
            request = request,
            targetLanguage = targetLanguage,
            providerId = fallbackId,
            llmChain = llmChain,
            runtimeSettings = runtimeSettings,
        )
    }

    suspend fun translateWithExactProvider(
        geminiApiKey: String,
        cerebrasApiKey: String,
        groqApiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
        providerId: String,
        llmChain: List<String>,
        runtimeSettings: PresetRuntimeSettings,
    ): TranslationExecutionResult = withContext(Dispatchers.IO) {
        val apiKeys = ApiKeys(
            geminiKey = geminiApiKey,
            cerebrasKey = cerebrasApiKey,
            groqKey = groqApiKey,
        )
        val response = dispatchProvider(
            providerId = providerId,
            apiKeys = apiKeys,
            request = request,
            targetLanguage = targetLanguage,
            llmChain = llmChain,
            runtimeSettings = runtimeSettings,
        )
        TranslationExecutionResult(providerId, response)
    }

    private suspend fun dispatchProvider(
        providerId: String,
        apiKeys: ApiKeys,
        request: TranslationRequest,
        targetLanguage: String,
        llmChain: List<String>,
        runtimeSettings: PresetRuntimeSettings,
    ): TranslationResponse {
        Log.d(
            TRANSLATION_TAG,
            "request provider=$providerId range=${request.sourceStart}-${request.sourceEnd} finalize=${request.bytesToCommit} draft=${request.draftSource.length}",
        )
        if (providerId == PROVIDER_GTX) {
            return translateWithGoogleGtx(request, targetLanguage)
        }
        return translateWithLlmChain(
            chainModelIds = llmChain,
            apiKeys = apiKeys,
            runtimeSettings = runtimeSettings,
            request = request,
            targetLanguage = targetLanguage,
        )
    }

    private suspend fun translateWithLlmChain(
        chainModelIds: List<String>,
        apiKeys: ApiKeys,
        runtimeSettings: PresetRuntimeSettings,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        var lastError: Throwable? = null
        for (modelId in chainModelIds) {
            val descriptor = PresetModelCatalog.getById(modelId) ?: continue
            if (!providerIsAvailable(descriptor.provider, apiKeys, runtimeSettings)) {
                continue
            }
            val attempt = runCatching {
                when (descriptor.provider) {
                    PresetModelProvider.CEREBRAS -> {
                        val key = apiKeys.cerebrasKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithCerebras(
                            endpoint = "https://api.cerebras.ai/v1/chat/completions",
                            apiKey = key,
                            model = descriptor.fullName,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    PresetModelProvider.GOOGLE -> {
                        val key = apiKeys.geminiKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithGemini(
                            endpoint = "https://generativelanguage.googleapis.com/v1beta/models/${descriptor.fullName}:generateContent",
                            apiKey = key,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    PresetModelProvider.GEMINI_LIVE -> {
                        val key = apiKeys.geminiKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithGeminiLive(
                            descriptor = descriptor,
                            apiKey = key,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    PresetModelProvider.GROQ -> {
                        val key = apiKeys.groqKey.takeIf { it.isNotBlank() }
                            ?: return@runCatching null
                        translateWithGroq(
                            apiKey = key,
                            model = descriptor.fullName,
                            request = request,
                            targetLanguage = targetLanguage,
                        )
                    }

                    else -> null
                }
            }
            val result = attempt.getOrElse {
                lastError = it
                null
            }
            if (result != null) {
                return result
            }
        }
        throw lastError ?: IOException("No LLM provider in the priority chain produced a translation.")
    }

    private suspend fun translateWithGeminiLive(
        descriptor: PresetModelDescriptor,
        apiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val text = httpClient.streamGeminiLiveText(
            model = descriptor,
            apiKey = apiKey,
            prompt = "",
            inputText = buildStructuredPrompt(request, targetLanguage),
            onChunk = {},
        )
        return parseTranslationResponse(text, request)
    }

    private fun translateWithCerebras(
        endpoint: String,
        apiKey: String,
        model: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        // Cerebras reasoning models reject strict json_schema; rely on prompt-driven JSON.
        val isReasoning = model.contains("gpt-oss") || model.contains("zai-glm")
        val payload = JSONObject()
            .put("model", model)
            .put("messages", cerebrasMessages(request, targetLanguage))
            .put("stream", false)
            .put("max_tokens", 512)
        if (!isReasoning) {
            payload.put("response_format", cerebrasResponseFormat())
        }
        val requestBody = payload.toString().toRequestBody(JSON_MEDIA_TYPE)

        val httpRequest = Request.Builder()
            .url(endpoint)
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Translation request failed with ${response.code}")
            }
            val body = response.body.string().orEmpty()
            val root = JSONObject(body)
            val jsonText = root.optJSONArray("choices")
                ?.optJSONObject(0)
                ?.optJSONObject("message")
                ?.optString("content")
                .orEmpty()
            return parseTranslationResponse(jsonText, request)
        }
    }

    private fun translateWithGroq(
        apiKey: String,
        model: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val requestBody = JSONObject()
            .put("model", model)
            .put("messages", cerebrasMessages(request, targetLanguage))
            .put("stream", false)
            .put("max_tokens", 512)
            .put("response_format", JSONObject().put("type", "json_object"))
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)

        val httpRequest = Request.Builder()
            .url("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", "Bearer $apiKey")
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Groq translation request failed with ${response.code}")
            }
            val body = response.body.string().orEmpty()
            val root = JSONObject(body)
            val jsonText = root.optJSONArray("choices")
                ?.optJSONObject(0)
                ?.optJSONObject("message")
                ?.optString("content")
                .orEmpty()
            return parseTranslationResponse(jsonText, request)
        }
    }

    private fun translateWithGemini(
        endpoint: String,
        apiKey: String,
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val requestBody = JSONObject()
            .put(
                "contents",
                JSONArray().put(
                    JSONObject()
                        .put("role", "user")
                        .put(
                            "parts",
                            JSONArray().put(
                                JSONObject().put("text", buildStructuredPrompt(request, targetLanguage)),
                            ),
                        ),
                ),
            )
            .put(
                "generationConfig",
                JSONObject().put("responseMimeType", "application/json"),
            )
            .toString()
            .toRequestBody(JSON_MEDIA_TYPE)

        val httpRequest = Request.Builder()
            .url(endpoint)
            .header("x-goog-api-key", apiKey)
            .header("Content-Type", "application/json")
            .post(requestBody)
            .build()

        httpClient.newCall(httpRequest).execute().use { response ->
            if (!response.isSuccessful) {
                throw IOException("Gemini translation request failed with ${response.code}")
            }
            val body = response.body.string().orEmpty()
            val root = JSONObject(body)
            val parts = root.optJSONArray("candidates")
                ?.optJSONObject(0)
                ?.optJSONObject("content")
                ?.optJSONArray("parts")
                ?: throw IOException("Gemini translation response body was empty.")
            val jsonText = buildString {
                for (index in 0 until parts.length()) {
                    append(parts.optJSONObject(index)?.optString("text").orEmpty())
                }
            }
            return parseTranslationResponse(jsonText, request)
        }
    }

    private fun translateWithGoogleGtx(
        request: TranslationRequest,
        targetLanguage: String,
    ): TranslationResponse {
        val patches = mutableListOf<TranslationPatch>()
        if (request.finalizedSource.isNotBlank()) {
            val translated = translateWithGoogleGtxText(
                text = request.finalizedSource,
                targetLanguage = targetLanguage,
            ) ?: error("GTX finalized translation failed.")
            patches += TranslationPatch(
                sourceStart = request.sourceStart,
                sourceEnd = request.finalizedSourceEnd,
                state = "final",
                translation = translated,
            )
        }
        if (request.draftSource.isNotBlank()) {
            val translated = translateWithGoogleGtxText(
                text = request.draftSource,
                targetLanguage = targetLanguage,
            ) ?: error("GTX draft translation failed.")
            patches += TranslationPatch(
                sourceStart = request.draftSourceStart,
                sourceEnd = request.sourceEnd,
                state = "draft",
                translation = translated,
            )
        }
        return validateTranslationResponse(TranslationResponse(patches), request)
    }

    private fun translateWithGoogleGtxText(
        text: String,
        targetLanguage: String,
    ): String? {
        val targetCode = LanguageCatalog.codeForName(targetLanguage).lowercase()
        val request = Request.Builder()
            .url(
                "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=$targetCode&dt=t&q=${java.net.URLEncoder.encode(text, "UTF-8")}",
            )
            .header("User-Agent", "Mozilla/5.0")
            .build()
        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                return null
            }
            val payload = response.body.string().orEmpty()
            val sentences = JSONArray(payload).optJSONArray(0) ?: return null
            return buildString {
                for (index in 0 until sentences.length()) {
                    append(sentences.optJSONArray(index)?.optString(0).orEmpty())
                }
            }.ifBlank { null }
        }
    }

    private companion object {
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
        private val PROVIDER_LLM = LiveTranslationModelCatalog.PROVIDER_LLM
        private val PROVIDER_GTX = LiveTranslationModelCatalog.PROVIDER_GTX
        private const val TRANSLATION_TAG = "LiveTranslate"
    }

    data class TranslationExecutionResult(
        val providerId: String,
        val response: TranslationResponse,
    )
}
