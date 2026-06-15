package dev.screengoated.toolbox.mobile.service.moonshine

/**
 * Available Zipformer streaming languages.
 * 7 dedicated per-language models + 1 multilingual (8-lang).
 *
 * Most models download individual files from HuggingFace.
 * Korean uses a ModelScope mirror (HF repo is gated).
 *
 * sherpaModelType defaults to "" (auto-detect from ONNX metadata).
 * Only override when the model's embedded metadata is known to be missing.
 */
enum class ZipformerLanguage(
    val code: String,
    val displayName: String,
    val modelName: String,
    val downloadBaseUrl: String,
    /** sherpa-onnx model type hint. Empty = auto-detect from ONNX metadata (safest). */
    val sherpaModelType: String = "",
) {
    ENGLISH("en", "English",
        "streaming-zipformer-en-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/main"),

    KOREAN("ko", "Korean",
        "streaming-zipformer-korean",
        "https://modelscope.cn/models/k2-fsa/sherpa-onnx-streaming-zipformer-korean-2024-06-16/resolve/master"),

    CHINESE("zh", "Chinese",
        "streaming-zipformer-zh",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30/resolve/main"),

    FRENCH("fr", "French",
        "streaming-zipformer-fr-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-fr-kroko-2025-08-06/resolve/main"),

    GERMAN("de", "German",
        "streaming-zipformer-de-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-de-kroko-2025-08-06/resolve/main"),

    SPANISH("es", "Spanish",
        "streaming-zipformer-es-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-es-kroko-2025-08-06/resolve/main"),

    RUSSIAN("ru", "Russian",
        "streaming-zipformer-small-ru-vosk",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-small-ru-vosk-2025-08-16/resolve/main"),

    ALL_8LANG("all-8", "AR, EN, ID, JA, RU, TH, VI, ZH",
        "streaming-zipformer-multilingual-8lang",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10/resolve/main");

    val modelFiles: List<String> get() = when (modelName) {
        "streaming-zipformer-en-kroko",
        "streaming-zipformer-fr-kroko",
        "streaming-zipformer-de-kroko",
        "streaming-zipformer-es-kroko" -> listOf(
            "encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt",
        )
        "streaming-zipformer-korean" -> listOf(
            "encoder-epoch-99-avg-1.int8.onnx",
            "decoder-epoch-99-avg-1.onnx",
            "joiner-epoch-99-avg-1.int8.onnx",
            "tokens.txt", "bpe.model",
        )
        "streaming-zipformer-zh" -> listOf(
            "encoder.int8.onnx", "decoder.onnx", "joiner.int8.onnx", "tokens.txt",
        )
        "streaming-zipformer-small-ru-vosk" -> listOf(
            "encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt", "bpe.model",
        )
        "streaming-zipformer-multilingual-8lang" -> listOf(
            "encoder-epoch-75-avg-11-chunk-16-left-128.int8.onnx",
            "decoder-epoch-75-avg-11-chunk-16-left-128.onnx",
            "joiner-epoch-75-avg-11-chunk-16-left-128.int8.onnx",
            "tokens.txt", "bpe.model",
        )
        else -> emptyList()
    }

    /**
     * True = model emits native punctuation (periods, ?, !) in streaming output.
     *        Uses sentence-boundary detection for commits (Cases 1 & 2).
     * False = model does not emit punctuation.
     *        Uses silence-based threshold commits with appended period (Case 3).
     *        Handled by the shared OfflineAsrStreamParity.commitStep machine.
     *
     * Confirmed by live testing on all 8 languages (2026-04):
     *   EN ✓  KO ✓  FR ✓  DE ✓  ES ✓  — native punctuation
     *   ZH ✗  RU ✗  All8 ✗            — no native punctuation
     *
     * Mirrors Windows `ZipformerLanguage::has_native_punctuation()`.
     */
    val hasNativePunctuation: Boolean get() = when (this) {
        ENGLISH, KOREAN, FRENCH, GERMAN, SPANISH -> true
        CHINESE, RUSSIAN, ALL_8LANG -> false
    }

    fun sherpaEncoder(): String = modelFiles.first { it.contains("encoder") }
    fun sherpaDecoder(): String = modelFiles.first { it.contains("decoder") }
    fun sherpaJoiner(): String = modelFiles.first { it.contains("joiner") }
    /** Returns "bpe.model" filename if this model needs BPE vocab, null otherwise. */
    val bpeVocabFile: String? get() = modelFiles.find { it == "bpe.model" }

    companion object {
        fun fromCode(code: String): ZipformerLanguage? =
            entries.find { it.code == code }
    }
}
