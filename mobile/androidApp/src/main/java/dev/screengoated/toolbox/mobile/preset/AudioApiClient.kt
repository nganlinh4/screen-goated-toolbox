package dev.screengoated.toolbox.mobile.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import java.io.IOException

internal data class AudioStreamingTranscriptResult(
    val transcript: String,
    val producedRealtimePaste: Boolean = false,
)

internal interface AudioStreamingSession {
    suspend fun appendPcm16Chunk(chunk: ShortArray)

    suspend fun finish(): AudioStreamingTranscriptResult

    fun cancel()
}

class AudioApiClient(
    internal val appContext: Context,
    internal val httpClient: OkHttpClient,
    internal val parakeetModelManager: ParakeetModelManager,
) {
    internal suspend fun openStreamingSession(
        modelId: String,
        _prompt: String,
        apiKeys: ApiKeys,
        uiLanguage: String,
        onChunk: (String) -> Unit,
    ): AudioStreamingSession? = withContext(Dispatchers.IO) {
        val model = resolveModel(modelId)
        when (model.provider) {
            PresetModelProvider.GEMINI_LIVE -> openGeminiLiveInputSession(
                model = model,
                apiKey = apiKeys.geminiKey,
                onChunk = onChunk,
            )

            PresetModelProvider.PARAKEET -> openParakeetStreamingSession(
                _model = model,
                _uiLanguage = uiLanguage,
                onChunk = onChunk,
            )

            else -> null
        }
    }

    suspend fun executeStreaming(
        modelId: String,
        prompt: String,
        wavBytes: ByteArray,
        apiKeys: ApiKeys,
        uiLanguage: String,
        onChunk: (String) -> Unit,
        streamingEnabled: Boolean = true,
    ): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val model = resolveModel(modelId)
            when (model.provider) {
                PresetModelProvider.GROQ -> transcribeWithGroq(
                    model = model,
                    wavBytes = wavBytes,
                    apiKey = apiKeys.groqKey,
                )

                PresetModelProvider.GOOGLE -> transcribeWithGemini(
                    model = model,
                    prompt = prompt,
                    wavBytes = wavBytes,
                    apiKey = apiKeys.geminiKey,
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.GEMINI_LIVE -> transcribeWithGeminiLiveInput(
                    model = model,
                    wavBytes = wavBytes,
                    apiKey = apiKeys.geminiKey,
                    onChunk = onChunk,
                )

                PresetModelProvider.PARAKEET -> transcribeWithParakeet(
                    model = model,
                    wavBytes = wavBytes,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                )

                else -> throw IOException("Unsupported audio provider: ${model.provider.name.lowercase()}")
            }
        }
    }

    internal fun resolveModel(modelId: String): PresetModelDescriptor {
        return requireNotNull(PresetModelCatalog.getById(modelId)) {
            "Unknown model config: $modelId"
        }
    }
}
