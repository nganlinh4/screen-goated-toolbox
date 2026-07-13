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

class VisionApiClient(internal val httpClient: OkHttpClient) {

    suspend fun executeStreaming(
        modelId: String,
        prompt: String,
        imageBytes: ByteArray,
        apiKeys: ApiKeys,
        uiLanguage: String,
        onChunk: (String) -> Unit,
        streamingEnabled: Boolean = true,
    ): Result<String> = withContext(Dispatchers.IO) {
        runCatching {
            val model = resolveModel(modelId)
            val prepared = prepareImage(
                rawBytes = imageBytes,
                provider = model.provider,
                promptBytes = prompt.toByteArray(Charsets.UTF_8).size,
            )
            when (model.provider) {
                PresetModelProvider.GOOGLE -> streamGeminiVision(
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    mimeType = prepared.mimeType,
                    apiKey = apiKeys.geminiKey,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
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
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.CEREBRAS -> streamOpenAiVision(
                    endpoint = CEREBRAS_ENDPOINT,
                    apiKey = apiKeys.cerebrasKey,
                    providerName = "Cerebras",
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    mimeType = prepared.mimeType,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
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
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.OLLAMA -> streamOllamaVision(
                    baseUrl = apiKeys.ollamaBaseUrl,
                    model = model,
                    prompt = prompt,
                    imageBase64 = prepared.base64,
                    uiLanguage = uiLanguage,
                    onChunk = onChunk,
                    streamingEnabled = streamingEnabled,
                )

                PresetModelProvider.QRSERVER -> callQrServer(
                    imageBytes = imageBytes,
                    onChunk = onChunk,
                )

                PresetModelProvider.GEMINI_LIVE -> httpClient.streamGeminiLiveVision(
                    model = model,
                    apiKey = apiKeys.geminiKey,
                    prompt = prompt,
                    imageBytes = prepared.bytes,
                    mimeType = prepared.mimeType,
                    onChunk = onChunk,
                )

                else ->
                    throw IOException("Unsupported vision provider: ${model.provider.name.lowercase()}")
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

internal fun prepareImage(
    rawBytes: ByteArray,
    provider: PresetModelProvider,
    promptBytes: Int,
): PreparedImage {
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
