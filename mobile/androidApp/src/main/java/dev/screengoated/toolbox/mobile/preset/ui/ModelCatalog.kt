package dev.screengoated.toolbox.mobile.preset.ui

import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.shared.preset.BlockType

enum class ModelProvider(val displayName: String, val icon: String) {
    GEMINI("Gemini", "\u2728"),
    CEREBRAS("Cerebras", "\u26A1"),
    GROQ("Groq", "\uD83D\uDD25"),
    GOOGLE_GTX("Google Translate", "\uD83C\uDF0D"),
    PARAKEET("Parakeet", "\uD83D\uDC26"),
    QR_SCANNER("QR Scanner", "\uD83D\uDD33"),
    OPENROUTER("OpenRouter", "\uD83C\uDF10"),
    OLLAMA("Ollama", "\uD83C\uDFE0"),
    GEMINI_LIVE("Gemini Live", "\uD83C\uDFA7"),
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

    val models: List<ModelEntry> = PresetModelCatalog.models.map(PresetModelDescriptor::toUiEntry)

    private val byId = models.associateBy { it.id }

    fun getById(id: String): ModelEntry? = byId[id]

    fun displayName(modelId: String): String =
        byId[modelId]?.let { "${it.provider.icon} ${it.displayName}" } ?: modelId

    fun forType(type: ModelType): List<ModelEntry> = models.filter { it.modelType == type }

    fun isNonLlm(modelId: String): Boolean = byId[modelId]?.isNonLlm == true

    fun forBlockType(blockType: BlockType): List<ModelEntry> {
        val targetType = when (blockType) {
            BlockType.IMAGE -> ModelType.VISION
            BlockType.AUDIO -> ModelType.AUDIO
            else -> ModelType.TEXT
        }
        return forType(targetType)
    }
}

private fun PresetModelDescriptor.toUiEntry(): ModelEntry {
    return ModelEntry(
        id = id,
        displayName = displayName,
        provider = provider.toUiProvider(),
        modelType = when (modelType) {
            PresetModelType.TEXT -> ModelType.TEXT
            PresetModelType.VISION -> ModelType.VISION
            PresetModelType.AUDIO -> ModelType.AUDIO
        },
        supportsSearch = PresetModelCatalog.supportsSearchByName(fullName),
        isNonLlm = isNonLlm,
    )
}

private fun PresetModelProvider.toUiProvider(): ModelProvider = when (this) {
    PresetModelProvider.GOOGLE -> ModelProvider.GEMINI
    PresetModelProvider.CEREBRAS -> ModelProvider.CEREBRAS
    PresetModelProvider.GROQ -> ModelProvider.GROQ
    PresetModelProvider.OPENROUTER -> ModelProvider.OPENROUTER
    PresetModelProvider.GOOGLE_GTX -> ModelProvider.GOOGLE_GTX
    PresetModelProvider.GEMINI_LIVE -> ModelProvider.GEMINI_LIVE
    PresetModelProvider.OLLAMA -> ModelProvider.OLLAMA
    PresetModelProvider.QRSERVER -> ModelProvider.QR_SCANNER
    PresetModelProvider.PARAKEET -> ModelProvider.PARAKEET
}
