package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import java.io.IOException

data class ApiKeys(
    val geminiKey: String = "",
    val cerebrasKey: String = "",
    val groqKey: String = "",
    val openRouterKey: String = "",
    val ollamaBaseUrl: String = "",
)

class TextApiClient(internal val httpClient: OkHttpClient) {

    suspend fun executeStreaming(
        modelId: String,
        prompt: String,
        inputText: String,
        apiKeys: ApiKeys,
        uiLanguage: String,
        searchLabel: String?,
        onChunk: (String) -> Unit,
        streamingEnabled: Boolean = true,
    ): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val model = resolveModel(modelId)
            when (model.provider) {
                PresetModelProvider.GOOGLE -> streamGemini(
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    apiKey = apiKeys.geminiKey,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.CEREBRAS -> streamCerebras(
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    apiKey = apiKeys.cerebrasKey,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.GROQ -> {
                    if (model.fullName.startsWith("groq/compound")) {
                        runGroqCompound(
                            apiKey = apiKeys.groqKey,
                            model = model,
                            prompt = prompt,
                            inputText = inputText,
                            searchLabel = searchLabel,
                            onChunk = onChunk,
                        )
                    } else {
                        streamOpenAiCompatible(
                            endpoint = GROQ_ENDPOINT,
                            apiKey = apiKeys.groqKey,
                            providerName = "Groq",
                            model = model,
                            prompt = prompt,
                            inputText = inputText,
                            uiLanguage = uiLanguage,
                            onChunk = onChunk,
                            streamingEnabled = streamingEnabled,
                        )
                    }
                }

                PresetModelProvider.OPENROUTER -> streamOpenAiCompatible(
                    endpoint = OPENROUTER_ENDPOINT,
                    apiKey = apiKeys.openRouterKey,
                    providerName = "OpenRouter",
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.GOOGLE_GTX -> translateGoogleGtx(
                    inputText = inputText,
                    prompt = prompt,
                    onChunk = onChunk,
                )

                PresetModelProvider.OLLAMA -> streamOllama(
                    baseUrl = apiKeys.ollamaBaseUrl,
                    model = model,
                    prompt = prompt,
                    inputText = inputText,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.GEMINI_LIVE ->
                    throw IOException("PROVIDER_NOT_READY:gemini-live")

                else ->
                    throw IOException("Unsupported text provider: ${model.provider.name.lowercase()}")
            }
        }
    }

    fun debugResolveTextRequest(modelId: String): ResolvedTextRequest {
        val model = resolveModel(modelId)
        return ResolvedTextRequest(
            modelId = model.id,
            provider = model.provider,
            apiModel = model.fullName,
            supportsSearch = PresetModelCatalog.supportsSearchByName(model.fullName),
            geminiThinkingConfig = PresetModelCatalog.geminiThinkingConfig(model.fullName),
        )
    }

    fun debugBuildRequestBody(
        modelId: String,
        prompt: String,
        inputText: String,
        streamingEnabled: Boolean = true,
    ): String {
        val model = resolveModel(modelId)
        return when (model.provider) {
            PresetModelProvider.GOOGLE -> buildGeminiDebugPayload(
                fullName = model.fullName,
                prompt = prompt,
                inputText = inputText,
                streamingEnabled = streamingEnabled,
            )

            PresetModelProvider.GROQ -> {
                if (model.fullName.startsWith("groq/compound")) {
                    buildGroqCompoundDebugPayload(
                        fullName = model.fullName,
                        prompt = prompt,
                        inputText = inputText,
                    )
                } else {
                    buildOpenAiCompatibleDebugPayload(
                        fullName = model.fullName,
                        prompt = prompt,
                        inputText = inputText,
                    )
                }
            }

            else -> buildOpenAiCompatibleDebugPayload(
                fullName = model.fullName,
                prompt = prompt,
                inputText = inputText,
            )
        }
    }

    internal fun resolveModel(modelId: String): PresetModelDescriptor {
        return requireNotNull(PresetModelCatalog.getById(modelId)) {
            "Unknown model config: $modelId"
        }
    }

    internal fun thinkingLabel(uiLanguage: String): String = when (uiLanguage) {
        "vi" -> "AI đang suy nghĩ..."
        "ko" -> "AI가 생각하는 중..."
        else -> "AI is thinking..."
    }

    companion object {
        const val WIPE_SIGNAL: String = "\u0000WIPE\u0000"
    }
}

data class ResolvedTextRequest(
    val modelId: String,
    val provider: PresetModelProvider,
    val apiModel: String,
    val supportsSearch: Boolean,
    val geminiThinkingConfig: Map<String, Any>?,
)
