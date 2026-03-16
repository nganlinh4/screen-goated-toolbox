package dev.screengoated.toolbox.mobile.preset.ui

/**
 * Catalog of AI models available in the preset node editor.
 * Mirrors the Windows model catalog from `src/config/model_config.rs`.
 */

enum class ModelProvider(val displayName: String, val icon: String) {
    GEMINI("Gemini", "\u2728"),           // ✨
    CEREBRAS("Cerebras", "\u26A1"),       // ⚡
    GROQ("Groq", "\uD83D\uDD25"),        // 🔥
    GOOGLE_GTX("Google Translate", "\uD83C\uDF0D"), // 🌍
    PARAKEET("Parakeet", "\uD83D\uDC26"), // 🐦
    QR_SCANNER("QR Scanner", "\uD83D\uDD33"), // 🔳
    OPENROUTER("OpenRouter", "\uD83C\uDF10"), // 🌐
    OLLAMA("Ollama", "\uD83C\uDFE0"),     // 🏠
}

enum class ModelType { TEXT, VISION, AUDIO }

data class ModelEntry(
    val id: String,
    val displayName: String,
    val provider: ModelProvider,
    val modelType: ModelType,
    val supportsSearch: Boolean = false,
    val isNonLlm: Boolean = false,
)

object ModelCatalog {

    val models: List<ModelEntry> = listOf(
        // --- Gemini (Vision + Text + Audio) ---
        ModelEntry("gemini-3.1-flash-lite-preview", "Gemini 3.1 Flash Lite", ModelProvider.GEMINI, ModelType.VISION),
        ModelEntry("gemini-3-flash-preview", "Gemini 3.0 Flash", ModelProvider.GEMINI, ModelType.VISION),
        ModelEntry("text_gemini_flash_lite", "Gemini Flash Lite (text)", ModelProvider.GEMINI, ModelType.TEXT),
        ModelEntry("text_gemini_3_0_flash", "Gemini 3.0 Flash (text)", ModelProvider.GEMINI, ModelType.TEXT, supportsSearch = true),
        ModelEntry("compound_mini", "Compound Mini", ModelProvider.GEMINI, ModelType.TEXT, supportsSearch = true),
        ModelEntry("gemini-audio", "Gemini Audio", ModelProvider.GEMINI, ModelType.AUDIO),
        ModelEntry("gemini-live-audio", "Gemini Live Audio", ModelProvider.GEMINI, ModelType.AUDIO),

        // --- Cerebras ---
        ModelEntry("cerebras_gpt_oss", "Cerebras GPT-OSS", ModelProvider.CEREBRAS, ModelType.TEXT),

        // --- Groq ---
        ModelEntry("groq_kimi", "Kimi (Groq)", ModelProvider.GROQ, ModelType.TEXT),
        ModelEntry("groq_llama_scout", "Llama Scout (Groq)", ModelProvider.GROQ, ModelType.TEXT),
        ModelEntry("groq_llama_maverick", "Llama Maverick (Groq)", ModelProvider.GROQ, ModelType.TEXT),
        ModelEntry("whisper-accurate", "Whisper Accurate", ModelProvider.GROQ, ModelType.AUDIO, isNonLlm = true),

        // --- Google GTX ---
        ModelEntry("google-gtx", "Google Translate", ModelProvider.GOOGLE_GTX, ModelType.TEXT, isNonLlm = true),

        // --- Parakeet (local) ---
        ModelEntry("parakeet-local", "Parakeet (local)", ModelProvider.PARAKEET, ModelType.AUDIO, isNonLlm = true),

        // --- QR Scanner ---
        ModelEntry("qr-scanner", "QR Scanner", ModelProvider.QR_SCANNER, ModelType.VISION, isNonLlm = true),

        // --- Google Gemma (Groq-hosted) ---
        ModelEntry("google-gemma", "Google Gemma", ModelProvider.GROQ, ModelType.TEXT),
    )

    private val byId = models.associateBy { it.id }

    fun getById(id: String): ModelEntry? = byId[id]

    fun displayName(modelId: String): String =
        byId[modelId]?.let { "${it.provider.icon} ${it.displayName}" } ?: modelId

    fun forType(type: ModelType): List<ModelEntry> = models.filter { it.modelType == type }

    fun isNonLlm(modelId: String): Boolean = byId[modelId]?.isNonLlm == true

    /** Models appropriate for a given block type */
    fun forBlockType(blockType: dev.screengoated.toolbox.mobile.shared.preset.BlockType): List<ModelEntry> {
        val targetType = when (blockType) {
            dev.screengoated.toolbox.mobile.shared.preset.BlockType.IMAGE -> ModelType.VISION
            dev.screengoated.toolbox.mobile.shared.preset.BlockType.AUDIO -> ModelType.AUDIO
            else -> ModelType.TEXT
        }
        return forType(targetType)
    }
}
