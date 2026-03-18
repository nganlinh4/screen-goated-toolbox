package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockType

enum class PresetModelProvider {
    GOOGLE,
    CEREBRAS,
    GROQ,
    OPENROUTER,
    GOOGLE_GTX,
    GEMINI_LIVE,
    OLLAMA,
    QRSERVER,
    PARAKEET,
}

enum class PresetModelType {
    TEXT,
    VISION,
    AUDIO,
}

data class PresetModelDescriptor(
    val id: String,
    val provider: PresetModelProvider,
    val fullName: String,
    val modelType: PresetModelType,
    val displayName: String,
    val nameVi: String = displayName,
    val nameKo: String = displayName,
    val isNonLlm: Boolean = false,
) {
    fun localizedName(lang: String): String = when (lang) {
        "vi" -> nameVi
        "ko" -> nameKo
        else -> displayName
    }
}

object PresetModelCatalog {
    val models: List<PresetModelDescriptor> = GeneratedPresetModelCatalogData.models

    private val byId = models.associateBy { it.id }

    fun getById(id: String): PresetModelDescriptor? = byId[id]

    fun forType(type: PresetModelType): List<PresetModelDescriptor> =
        models.filter { it.modelType == type }

    fun forBlockType(blockType: BlockType): List<PresetModelDescriptor> {
        val targetType = when (blockType) {
            BlockType.IMAGE -> PresetModelType.VISION
            BlockType.AUDIO -> PresetModelType.AUDIO
            else -> PresetModelType.TEXT
        }
        return forType(targetType)
    }

    fun isNonLlm(id: String): Boolean = getById(id)?.isNonLlm == true

    fun supportsSearchById(id: String): Boolean = getById(id)?.let { supportsSearchByName(it.fullName) } ?: false

    fun supportsSearchByName(fullName: String): Boolean {
        if (fullName.contains("gemma-3-27b-it")) {
            return false
        }
        if (fullName.contains("gemini-3-flash-preview") || fullName.contains("gemini-3.1-flash-lite-preview")) {
            return false
        }
        if (fullName.contains("gemini")) {
            return true
        }
        if (fullName.contains("gemma")) {
            return false
        }
        if (fullName.contains("compound")) {
            return true
        }
        return false
    }

    fun geminiThinkingConfig(fullName: String): Map<String, Any>? {
        if (fullName.contains("gemini-3.1-flash-lite")) {
            return mapOf("thinkingLevel" to "MINIMAL")
        }

        val supportsThinking = (fullName.contains("gemini-2.5-flash") && !fullName.contains("lite")) ||
            fullName.contains("gemini-3-flash-preview") ||
            fullName.contains("gemini-robotics")

        return if (supportsThinking) {
            mapOf("includeThoughts" to true)
        } else {
            null
        }
    }
}
