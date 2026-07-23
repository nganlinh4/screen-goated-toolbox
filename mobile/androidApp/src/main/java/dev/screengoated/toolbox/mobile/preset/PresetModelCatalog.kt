package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockType

import kotlinx.serialization.Serializable

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
    MOONSHINE,
    TAALAS,
}

enum class PresetModelType {
    TEXT,
    VISION,
    AUDIO,
}

enum class PresetModelSource {
    BUILT_IN,
    USER,
    DISCOVERED,
}

@Serializable
data class CustomPresetModelDefinition(
    val id: String,
    val provider: PresetModelProvider = PresetModelProvider.OPENROUTER,
    val displayName: String,
    val fullName: String,
    val modelType: PresetModelType = PresetModelType.TEXT,
    val enabled: Boolean = true,
    val quotaEn: String = "Provider quota",
    val quotaVi: String = "Theo nhà cung cấp",
    val quotaKo: String = "공급자 기준",
    val supportsSearch: Boolean? = null,
)

data class PresetModelDescriptor(
    val id: String,
    val provider: PresetModelProvider,
    val fullName: String,
    val modelType: PresetModelType,
    val displayName: String,
    val nameVi: String = displayName,
    val nameKo: String = displayName,
    val isNonLlm: Boolean = false,
    val quotaEn: String = "",
    val quotaVi: String = "",
    val quotaKo: String = "",
    val source: PresetModelSource = PresetModelSource.BUILT_IN,
    val supportsSearchOverride: Boolean? = null,
    val qualityTier: Int? = null,
    val typicalLatencyMs: Int? = null,
    val performanceSource: String? = null,
) {
    fun localizedName(lang: String): String = when (lang) {
        "vi" -> nameVi
        "ko" -> nameKo
        else -> displayName
    }

    fun localizedQuota(lang: String): String = when (lang) {
        "vi" -> quotaVi
        "ko" -> quotaKo
        else -> quotaEn
    }
}

object PresetCustomModelRegistry {
    @Volatile
    private var customModels: List<CustomPresetModelDefinition> = emptyList()

    fun set(models: List<CustomPresetModelDefinition>) {
        customModels = models
    }

    fun definitions(): List<CustomPresetModelDefinition> = customModels

    fun descriptors(): List<PresetModelDescriptor> = customModels.mapNotNull { model ->
        if (model.id.isBlank() || model.fullName.isBlank() || !model.enabled) {
            null
        } else {
            PresetModelDescriptor(
                id = model.id,
                provider = model.provider,
                fullName = model.fullName,
                modelType = model.modelType,
                displayName = model.displayName.ifBlank { model.fullName },
                nameVi = model.displayName.ifBlank { model.fullName },
                nameKo = model.displayName.ifBlank { model.fullName },
                quotaEn = model.quotaEn,
                quotaVi = model.quotaVi,
                quotaKo = model.quotaKo,
                source = PresetModelSource.USER,
                supportsSearchOverride = model.supportsSearch,
            )
        }
    }
}

object PresetModelCatalog {
    private val builtInModels: List<PresetModelDescriptor> = GeneratedPresetModelCatalogData.models
    private val allModels: List<PresetModelDescriptor>
        get() = builtInModels + PresetCustomModelRegistry.descriptors()
    val models: List<PresetModelDescriptor>
        get() = allModels.filter { it.provider != PresetModelProvider.PARAKEET }

    private val byId: Map<String, PresetModelDescriptor>
        get() = allModels.associateBy { it.id }

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

    fun dialogModels(): List<PresetModelDescriptor> = models

    fun isNonLlm(id: String): Boolean = getById(id)?.isNonLlm == true

    fun supportsSearchById(id: String): Boolean = getById(id)?.let {
        it.supportsSearchOverride ?: supportsSearchByName(it.fullName)
    } ?: false

    fun supportsSearchByName(fullName: String): Boolean {
        if (fullName in GeneratedPresetModelCatalogData.searchDisabledFullNames) {
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
        if (fullName.contains("gemini-3.1-flash-lite") ||
            fullName.contains("gemini-3.5-flash-lite") ||
            fullName.contains("gemma-4-")
        ) {
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
