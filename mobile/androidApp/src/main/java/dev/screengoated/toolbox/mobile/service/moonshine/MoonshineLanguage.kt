package dev.screengoated.toolbox.mobile.service.moonshine

/**
 * Moonshine Voice model variants (English-only streaming).
 */
enum class MoonshineLanguage(
    val code: String,
    val displayName: String,
    val modelName: String,
    val moonshineArch: Int,
    val downloadBaseUrl: String,
) {
    ENGLISH_TINY("en", "English (Tiny)",
        "tiny-streaming-en", 2,
        "https://download.moonshine.ai/model/tiny-streaming-en/quantized"),

    ENGLISH_SMALL("en", "English (Small)",
        "small-streaming-en", 4,
        "https://download.moonshine.ai/model/small-streaming-en/quantized"),

    ENGLISH_MEDIUM("en", "English (Medium)",
        "medium-streaming-en", 5,
        "https://download.moonshine.ai/model/medium-streaming-en/quantized");

    val modelFiles: List<String> = listOf(
        "adapter.ort", "cross_kv.ort", "decoder_kv.ort",
        "encoder.ort", "frontend.ort", "streaming_config.json", "tokenizer.bin",
    )

    companion object {
        fun forModelId(modelId: String): MoonshineLanguage = when (modelId) {
            "moonshine-small-streaming" -> ENGLISH_SMALL
            "moonshine-medium-streaming" -> ENGLISH_MEDIUM
            else -> ENGLISH_TINY
        }
    }
}
