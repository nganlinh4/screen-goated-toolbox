package dev.screengoated.toolbox.mobile.preset

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
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
            val prepared = prepareImage(imageBytes)
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

    companion object {
        private const val MAX_DIMENSION = 2048
    }
}

internal data class PreparedImage(
    val bytes: ByteArray,
    val base64: String,
    val mimeType: String,
)

internal fun prepareImage(rawBytes: ByteArray): PreparedImage {
    val mimeType = sniffMimeType(rawBytes)
    val bitmap = BitmapFactory.decodeByteArray(rawBytes, 0, rawBytes.size)
        ?: throw IOException("Failed to decode image bytes")

    val resized = if (bitmap.width > 2048 || bitmap.height > 2048) {
        val (newW, newH) = if (bitmap.width > bitmap.height) {
            val ratio = 2048f / bitmap.width
            2048 to (bitmap.height * ratio).toInt()
        } else {
            val ratio = 2048f / bitmap.height
            (bitmap.width * ratio).toInt() to 2048
        }
        val scaled = Bitmap.createScaledBitmap(bitmap, newW, newH, true)
        if (scaled !== bitmap) bitmap.recycle()
        scaled
    } else {
        bitmap
    }

    val pngBytes = ByteArrayOutputStream().use { out ->
        resized.compress(Bitmap.CompressFormat.PNG, 100, out)
        if (resized !== bitmap) resized.recycle()
        out.toByteArray()
    }

    val base64 = Base64.encodeToString(pngBytes, Base64.NO_WRAP)
    return PreparedImage(
        bytes = pngBytes,
        base64 = base64,
        mimeType = if (mimeType != "image/png") "image/png" else mimeType,
    )
}

internal fun sniffMimeType(bytes: ByteArray): String {
    if (bytes.size < 12) return "image/png"
    if (bytes[0] == 0xFF.toByte() && bytes[1] == 0xD8.toByte() && bytes[2] == 0xFF.toByte()) {
        return "image/jpeg"
    }
    if (bytes[0] == 0x89.toByte() && bytes[1] == 0x50.toByte() &&
        bytes[2] == 0x4E.toByte() && bytes[3] == 0x47.toByte()
    ) {
        return "image/png"
    }
    if (bytes[0] == 0x52.toByte() && bytes[1] == 0x49.toByte() &&
        bytes[2] == 0x46.toByte() && bytes[3] == 0x46.toByte() &&
        bytes.size >= 12 &&
        bytes[8] == 0x57.toByte() && bytes[9] == 0x45.toByte() &&
        bytes[10] == 0x42.toByte() && bytes[11] == 0x50.toByte()
    ) {
        return "image/webp"
    }
    return "image/png"
}
