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

enum class PresetReasoningPolicy {
    NOT_APPLICABLE,
    GEMINI_DISABLED,
    GEMINI_MINIMAL,
    OPENAI_NONE,
    OPENAI_LOW,
    PROVIDER_MANAGED,
    LIVE_PROFILE,
}

enum class PresetVisionInputOrder {
    TEXT_FIRST,
    IMAGE_FIRST,
}

enum class PresetVisionMediaResolution {
    PROVIDER_DEFAULT,
}

enum class PresetVisionSamplingPolicy {
    PROVIDER_DEFAULT,
    QWEN3_GROQ_NON_THINKING,
}

enum class PresetStructuredOutputPolicy {
    UNSUPPORTED,
    PROMPT_ONLY,
    JSON_OBJECT,
    STRICT_JSON_SCHEMA,
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
    val searchToolEnabledByDefault: Boolean = false,
    val reasoningPolicy: PresetReasoningPolicy = PresetReasoningPolicy.NOT_APPLICABLE,
    val visionInputOrder: PresetVisionInputOrder = PresetVisionInputOrder.TEXT_FIRST,
    val visionMediaResolution: PresetVisionMediaResolution =
        PresetVisionMediaResolution.PROVIDER_DEFAULT,
    val visionSamplingPolicy: PresetVisionSamplingPolicy =
        PresetVisionSamplingPolicy.PROVIDER_DEFAULT,
    val visionMaxOutputTokens: Int? = null,
    val structuredOutputPolicy: PresetStructuredOutputPolicy =
        PresetStructuredOutputPolicy.UNSUPPORTED,
    val intelligenceTier: Int? = null,
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
    private val selectableModels: List<PresetModelDescriptor>
        get() = allModels.filter { it.provider != PresetModelProvider.PARAKEET }
    val models: List<PresetModelDescriptor>
        get() = selectableModels.sortedWith(displayComparator)

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
    fun runtimeModels(): List<PresetModelDescriptor> = selectableModels

    fun isNonLlm(id: String): Boolean = getById(id)?.isNonLlm == true

    fun supportsSearchById(id: String): Boolean = getById(id)?.let {
        it.supportsSearchOverride ?: false
    } ?: false

    fun searchToolEnabledByDefaultById(id: String): Boolean =
        getById(id)?.searchToolEnabledByDefault == true

    fun supportsSearch(
        provider: PresetModelProvider,
        fullName: String,
    ): Boolean = modelProfile(provider, fullName)?.supportsSearchOverride ?: false

    fun geminiThinkingConfig(
        provider: PresetModelProvider,
        fullName: String,
    ): Map<String, Any>? =
        when (reasoningPolicy(provider, fullName)) {
            PresetReasoningPolicy.GEMINI_DISABLED -> mapOf("thinkingBudget" to 0)
            PresetReasoningPolicy.GEMINI_MINIMAL -> mapOf("thinkingLevel" to "MINIMAL")
            else -> null
        }

    fun geminiImportantTaskThinkingConfig(
        provider: PresetModelProvider,
        fullName: String,
    ): Map<String, Any>? =
        if (reasoningPolicy(provider, fullName) == PresetReasoningPolicy.GEMINI_MINIMAL) {
            mapOf("thinkingLevel" to "LOW")
        } else {
            null
        }

    fun openAiReasoningEffort(
        provider: PresetModelProvider,
        fullName: String,
    ): String? =
        when (reasoningPolicy(provider, fullName)) {
            PresetReasoningPolicy.OPENAI_NONE -> "none"
            PresetReasoningPolicy.OPENAI_LOW -> "low"
            else -> null
        }

    private fun reasoningPolicy(
        provider: PresetModelProvider,
        fullName: String,
    ): PresetReasoningPolicy =
        modelProfile(provider, fullName)?.reasoningPolicy
            ?: PresetReasoningPolicy.NOT_APPLICABLE

    private fun modelProfile(
        provider: PresetModelProvider,
        fullName: String,
    ): PresetModelDescriptor? =
        allModels.firstOrNull { it.provider == provider && it.fullName == fullName }

    private val displayComparator =
        compareBy<PresetModelDescriptor>(
            { it.typicalLatencyMs ?: Int.MAX_VALUE },
            PresetModelDescriptor::id,
        )
}
