package dev.screengoated.toolbox.mobile.preset

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import androidx.core.graphics.scale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import java.io.ByteArrayOutputStream
import java.io.IOException
import org.json.JSONObject

class VisionApiClient(internal val httpClient: OkHttpClient) {

    suspend fun executeStreaming(
        modelId: String,
        prompt: String,
        imageBytes: ByteArray,
        apiKeys: ApiKeys,
        uiLanguage: String,
        onChunk: (String) -> Unit,
        streamingEnabled: Boolean = true,
        responseSchema: JSONObject? = null,
    ): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val model = resolveModel(modelId)
            require(model.modelType == PresetModelType.VISION && model.provider.hasVisionPresetRuntime()) {
                "Unsupported vision provider: ${model.provider.name.lowercase()}"
            }
            val normalizer = InitialLineBreakNormalizer()
            val repetitionGuard = model.restatesOutput.takeIf { it }?.let { VisionRepetitionGuard() }
            val guardedOnChunk: (String) -> Unit = guarded@{ rawChunk ->
                val chunk = normalizer.observe(rawChunk) ?: return@guarded
                if (chunk.startsWith(TextApiClient.WIPE_SIGNAL)) {
                    val replacement = chunk.removePrefix(TextApiClient.WIPE_SIGNAL)
                    repetitionGuard?.restart(replacement)
                    onChunk(chunk)
                } else if (repetitionGuard == null) {
                    onChunk(chunk)
                } else {
                    when (val action = repetitionGuard.observe(chunk)) {
                        RepetitionAction.Paint -> onChunk(chunk)
                        is RepetitionAction.Replace ->
                            onChunk("${TextApiClient.WIPE_SIGNAL}${action.text}")
                        RepetitionAction.Suppress -> Unit
                    }
                }
            }
            val prepared = prepareImage(
                rawBytes = imageBytes,
                provider = model.provider,
                modelFullName = model.fullName,
                promptBytes = prompt.toByteArray(Charsets.UTF_8).size,
            )
            val result = when (model.provider) {
                PresetModelProvider.GOOGLE -> streamGeminiVision(
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    mimeType = prepared.mimeType,
                    apiKey = apiKeys.geminiKey,
                    uiLanguage = uiLanguage,
                    onChunk = guardedOnChunk,
                    streamingEnabled = streamingEnabled,
                    responseSchema = responseSchema,
                )

                PresetModelProvider.GROQ -> streamOpenAiVision(
                    endpoint = GROQ_ENDPOINT,
                    apiKey = apiKeys.groqKey,
                    providerName = "Groq",
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    mimeType = prepared.mimeType,
                    uiLanguage = uiLanguage,
                    onChunk = guardedOnChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.NVIDIA -> streamOpenAiVision(
                    endpoint = NVIDIA_ENDPOINT,
                    apiKey = apiKeys.nvidiaKey,
                    providerName = "NVIDIA",
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    mimeType = prepared.mimeType,
                    uiLanguage = uiLanguage,
                    onChunk = guardedOnChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.OPENROUTER -> streamOpenAiVision(
                    endpoint = OPENROUTER_ENDPOINT,
                    apiKey = apiKeys.openRouterKey,
                    providerName = "OpenRouter",
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    mimeType = prepared.mimeType,
                    uiLanguage = uiLanguage,
                    onChunk = guardedOnChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.OLLAMA -> streamOllamaVision(
                    baseUrl = apiKeys.ollamaBaseUrl,
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    uiLanguage = uiLanguage,
                    onChunk = guardedOnChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.QRSERVER -> callQrServer(
                    imageBytes = imageBytes,
                    onChunk = guardedOnChunk,
                )

                PresetModelProvider.GEMINI_LIVE -> httpClient.streamGeminiLiveVision(
                    model = model,
                    apiKey = apiKeys.geminiKey,
                    prompt = prompt,
                    imageBytes = prepared.bytes,
                    mimeType = prepared.mimeType,
                    onChunk = guardedOnChunk,
                )

                else ->
                    throw IOException("Unsupported vision provider: ${model.provider.name.lowercase()}")
            }
            val normalized = normalizer.finish(result)
            if (repetitionGuard == null) {
                normalized
            } else {
                val salvaged = repetitionGuard.finish(normalized)
                if (salvaged != normalized) {
                    onChunk("${TextApiClient.WIPE_SIGNAL}$salvaged")
                }
                salvaged
            }
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

}

internal data class PreparedImage(
    val bytes: ByteArray,
    val base64: String,
    val mimeType: String,
)

private const val MAX_DIMENSION = 2048
private const val GROQ_SAFE_REQUEST_BYTES = 3_800_000
private const val GROQ_JSON_RESERVE_BYTES = 16_384
private const val GROQ_MAX_IMAGE_BYTES = 2_500_000
private const val GROQ_MIN_IMAGE_BYTES = 262_144
private val GROQ_JPEG_QUALITIES = intArrayOf(90, 82, 74, 66, 58)
private val GROQ_RESIZE_DIMENSIONS = intArrayOf(2048, 1792, 1536, 1280, 1024, 768)
private const val QWEN_PORTABLE_TPM_LIMIT = 8_000
private const val QWEN_IMAGE_AND_ENVELOPE_TOKEN_RESERVE = 3_072
private const val QWEN_ESTIMATED_PROMPT_BYTES_PER_TOKEN = 3

internal fun prepareImage(
    rawBytes: ByteArray,
    provider: PresetModelProvider,
    modelFullName: String,
    promptBytes: Int,
): PreparedImage {
    val requestProfile = PresetModelCatalog.runtimeModels().firstOrNull {
        it.provider == provider && it.fullName == modelFullName
    }
    if (
        provider == PresetModelProvider.GROQ &&
        requestProfile?.visionSamplingPolicy ==
            PresetVisionSamplingPolicy.QWEN3_GROQ_NON_THINKING
    ) {
        val completionReserve = requireNotNull(requestProfile.visionMaxOutputTokens) {
            "Qwen Groq vision request profile has no output-token limit"
        }
        ensureQwenPromptFitsPortableTpm(promptBytes, completionReserve)
    }
    val bitmap = BitmapFactory.decodeByteArray(rawBytes, 0, rawBytes.size)
        ?: throw IOException("Failed to decode image bytes")
    val resized = resizeToMax(bitmap, MAX_DIMENSION)
    if (resized !== bitmap) bitmap.recycle()

    val pngBytes = encodeBitmap(resized, Bitmap.CompressFormat.PNG, 100)
    if (provider != PresetModelProvider.GROQ) {
        resized.recycle()
        return preparedImage(pngBytes, "image/png")
    }

    val budget = groqImageByteBudget(promptBytes)
    if (pngBytes.size <= budget) {
        resized.recycle()
        return preparedImage(pngBytes, "image/png")
    }

    for (maxDimension in GROQ_RESIZE_DIMENSIONS) {
        val candidate = resizeToMax(resized, maxDimension)
        for (quality in GROQ_JPEG_QUALITIES) {
            val jpegBytes = encodeBitmap(candidate, Bitmap.CompressFormat.JPEG, quality)
            if (jpegBytes.size <= budget) {
                if (candidate !== resized) candidate.recycle()
                resized.recycle()
                return preparedImage(jpegBytes, "image/jpeg")
            }
        }
        if (candidate !== resized) candidate.recycle()
    }

    resized.recycle()
    throw IOException("Groq vision image cannot fit the safe request-size budget")
}

internal fun groqImageByteBudget(promptBytes: Int): Int {
    val availableBase64 = GROQ_SAFE_REQUEST_BYTES - GROQ_JSON_RESERVE_BYTES - promptBytes
    val rawBudget = availableBase64 / 4 * 3
    if (rawBudget < GROQ_MIN_IMAGE_BYTES) {
        throw IOException("Prompt leaves too little room for a Groq vision image")
    }
    return minOf(rawBudget, GROQ_MAX_IMAGE_BYTES)
}

internal fun ensureQwenPromptFitsPortableTpm(
    promptBytes: Int,
    completionTokenReserve: Int,
) {
    val estimatedPromptTokens =
        (promptBytes + QWEN_ESTIMATED_PROMPT_BYTES_PER_TOKEN - 1) /
            QWEN_ESTIMATED_PROMPT_BYTES_PER_TOKEN
    val estimatedRequestTokens =
        estimatedPromptTokens +
            completionTokenReserve +
            QWEN_IMAGE_AND_ENVELOPE_TOKEN_RESERVE
    if (estimatedRequestTokens > QWEN_PORTABLE_TPM_LIMIT) {
        throw IOException(
            "Qwen Groq vision prompt is too large for the portable " +
                "$QWEN_PORTABLE_TPM_LIMIT-TPM request budget " +
                "(estimated $estimatedRequestTokens tokens)",
        )
    }
}

private fun resizeToMax(bitmap: Bitmap, maxDimension: Int): Bitmap {
    if (bitmap.width <= maxDimension && bitmap.height <= maxDimension) return bitmap
    val ratio = maxDimension.toFloat() / maxOf(bitmap.width, bitmap.height)
    val width = (bitmap.width * ratio).toInt().coerceAtLeast(1)
    val height = (bitmap.height * ratio).toInt().coerceAtLeast(1)
    return bitmap.scale(width, height)
}

private fun encodeBitmap(bitmap: Bitmap, format: Bitmap.CompressFormat, quality: Int): ByteArray {
    return ByteArrayOutputStream().use { output ->
        if (!bitmap.compress(format, quality, output)) {
            throw IOException("Failed to encode vision image")
        }
        output.toByteArray()
    }
}

private fun preparedImage(bytes: ByteArray, mimeType: String): PreparedImage {
    return PreparedImage(
        bytes = bytes,
        base64 = Base64.encodeToString(bytes, Base64.NO_WRAP),
        mimeType = mimeType,
    )
}
