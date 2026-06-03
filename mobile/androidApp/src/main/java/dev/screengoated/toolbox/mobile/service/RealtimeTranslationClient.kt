package dev.screengoated.toolbox.mobile.service

import android.util.Base64
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
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.json.JSONException
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
            val body = response.body?.string().orEmpty()
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
            val body = response.body?.string().orEmpty()
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
            val body = response.body?.string().orEmpty()
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

    private fun cerebrasMessages(
        request: TranslationRequest,
        targetLanguage: String,
    ): JSONArray {
        val messages = JSONArray()
        messages.put(
            JSONObject()
                .put("role", "system")
                .put(
                    "content",
                    "You translate live transcript windows into JSON source patches. Respond with JSON only.",
                ),
        )
        request.history.forEach { entry ->
            messages.put(
                JSONObject()
                    .put("role", "user")
                    .put("content", "Translate to $targetLanguage:\n${entry.source}"),
            )
            messages.put(
                JSONObject()
                    .put("role", "assistant")
                    .put("content", entry.translation),
            )
        }
        messages.put(
            JSONObject()
                .put("role", "user")
                .put("content", buildStructuredPrompt(request, targetLanguage)),
        )
        return messages
    }

    private fun buildStructuredPrompt(
        request: TranslationRequest,
        targetLanguage: String,
    ): String {
        val history = JSONArray().apply {
            request.history.forEach { entry ->
                put(
                    JSONObject()
                        .put("source", entry.source)
                        .put("translation", entry.translation),
                )
            }
        }
        val expectedPatches = JSONArray().apply {
            if (request.finalizedSource.isNotBlank()) {
                put(
                    JSONObject()
                        .put("sourceStart", request.sourceStart)
                        .put("sourceEnd", request.finalizedSourceEnd)
                        .put("state", "final"),
                )
            }
            if (request.draftSource.isNotBlank()) {
                put(
                    JSONObject()
                        .put("sourceStart", request.draftSourceStart)
                        .put("sourceEnd", request.sourceEnd)
                        .put("state", "draft"),
                )
            }
        }
        val window = JSONObject()
            .put("sourceStart", request.sourceStart)
            .put("sourceEnd", request.sourceEnd)
            .put("pendingSource", request.pendingSource)
            .put("finalizedSource", request.finalizedSource)
            .put("draftSource", request.draftSource)
            .put("previousDraftTranslation", request.previousDraftTranslation)

        return buildString {
            append("You are a professional live translator.\n")
            append("Translate only the provided source window into ")
            append(targetLanguage)
            append(".\n")
            append("Return JSON with a single key named patches.\n")
            append("Each patch must keep the exact sourceStart/sourceEnd values from expectedPatches.\n")
            append("Use state=\"final\" for the finalized source span and state=\"draft\" for the trailing unfinished span.\n")
            append("Do not add commentary, markdown, or extra keys.\n\n")
            append("Recent committed context:\n")
            append(history.toString())
            append("\n\n")
            append("Current source window:\n")
            append(window.toString())
            append("\n\n")
            append("Expected patches:\n")
            append(expectedPatches.toString())
        }
    }

    private fun cerebrasResponseFormat(): JSONObject {
        val patchSchema = JSONObject()
            .put("type", "object")
            .put(
                "properties",
                JSONObject()
                    .put("sourceStart", JSONObject().put("type", "integer"))
                    .put("sourceEnd", JSONObject().put("type", "integer"))
                    .put(
                        "state",
                        JSONObject()
                            .put("type", "string")
                            .put("enum", JSONArray().put("final").put("draft")),
                    )
                    .put("translation", JSONObject().put("type", "string")),
            )
            .put(
                "required",
                JSONArray()
                    .put("sourceStart")
                    .put("sourceEnd")
                    .put("state")
                    .put("translation"),
            )
            .put("additionalProperties", false)

        val schema = JSONObject()
            .put("type", "object")
            .put(
                "properties",
                JSONObject().put(
                    "patches",
                    JSONObject()
                        .put("type", "array")
                        .put("items", patchSchema),
                ),
            )
            .put("required", JSONArray().put("patches"))
            .put("additionalProperties", false)

        return JSONObject()
            .put("type", "json_schema")
            .put(
                "json_schema",
                JSONObject()
                    .put("name", "live_translate_patches")
                    .put("strict", true)
                    .put("schema", schema),
            )
    }

    private fun parseTranslationResponse(
        payload: String,
        request: TranslationRequest,
    ): TranslationResponse {
        if (payload.isBlank()) {
            throw IOException("Translation response payload was empty.")
        }
        try {
            val root = JSONObject(payload)
            val patchesJson = root.optJSONArray("patches")
                ?: throw IOException("Translation response did not include patches.")
            val patches = buildList {
                for (index in 0 until patchesJson.length()) {
                    val patch = patchesJson.optJSONObject(index) ?: continue
                    add(
                        TranslationPatch(
                            sourceStart = patch.optInt("sourceStart", Int.MIN_VALUE),
                            sourceEnd = patch.optInt("sourceEnd", Int.MIN_VALUE),
                            state = patch.optString("state"),
                            translation = patch.optString("translation"),
                        ),
                    )
                }
            }
            return validateTranslationResponse(TranslationResponse(patches), request)
        } catch (error: JSONException) {
            throw IOException("Translation response was not valid JSON.", error)
        }
    }

    private fun validateTranslationResponse(
        response: TranslationResponse,
        request: TranslationRequest,
    ): TranslationResponse {
        val expectedPatches = buildList<Triple<Int, Int, String>> {
            if (request.finalizedSource.isNotBlank()) {
                add(Triple(request.sourceStart, request.finalizedSourceEnd, "final"))
            }
            if (request.draftSource.isNotBlank()) {
                add(Triple(request.draftSourceStart, request.sourceEnd, "draft"))
            }
        }
        val normalized = mutableListOf<TranslationPatch>()
        expectedPatches.forEach { expected ->
            val patch = response.patches.firstOrNull { candidate ->
                candidate.sourceStart == expected.first &&
                    candidate.sourceEnd == expected.second &&
                    candidate.state == expected.third &&
                    candidate.translation.isNotBlank()
            }
            if (patch != null) {
                normalized += patch.copy(translation = patch.translation.trim())
                return@forEach
            }

            if (expected.third == "draft" && !request.requiresDraftTranslation()) {
                normalized += TranslationPatch(
                    sourceStart = expected.first,
                    sourceEnd = expected.second,
                    state = expected.third,
                    translation = request.fallbackDraftTranslation(),
                )
                return@forEach
            }

            throw IOException(
                "Translation response missing expected ${expected.third} patch ${expected.first}-${expected.second}.",
            )
        }
        return TranslationResponse(normalized)
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
            val payload = response.body?.string().orEmpty()
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
