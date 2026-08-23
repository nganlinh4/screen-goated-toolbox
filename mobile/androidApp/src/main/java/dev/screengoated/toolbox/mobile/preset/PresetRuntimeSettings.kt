package dev.screengoated.toolbox.mobile.preset

import kotlinx.serialization.Serializable

@Serializable
data class PresetProviderSettings(
    val useGroq: Boolean = GeneratedPresetModelCatalogData.providerSettings.useGroq,
    val useGemini: Boolean = GeneratedPresetModelCatalogData.providerSettings.useGemini,
    val useOpenRouter: Boolean = GeneratedPresetModelCatalogData.providerSettings.useOpenRouter,
    val useNvidia: Boolean = GeneratedPresetModelCatalogData.providerSettings.useNvidia,
    val useOllama: Boolean = GeneratedPresetModelCatalogData.providerSettings.useOllama,
)

internal fun presetProviderEnabled(
    provider: PresetModelProvider,
    settings: PresetProviderSettings,
): Boolean = when (provider) {
    PresetModelProvider.GROQ -> settings.useGroq
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GEMINI_LIVE,
    -> settings.useGemini
    PresetModelProvider.OPENROUTER -> settings.useOpenRouter
    PresetModelProvider.NVIDIA -> settings.useNvidia
    PresetModelProvider.OLLAMA -> settings.useOllama
    else -> true
}

@Serializable
data class PresetModelPriorityChains(
    val imageToText: List<String> = GeneratedPresetModelCatalogData.modelPriorityChains.imageToText,
    val textToText: List<String> = GeneratedPresetModelCatalogData.modelPriorityChains.textToText,
)

@Serializable
data class PresetLiveModelOverrides(
    val pinned: List<String> = emptyList(),
    val excluded: List<String> = emptyList(),
)

@Serializable
data class PresetAdaptiveModelPriority(
    val imageToText: Boolean = true,
    val textToText: Boolean = true,
    val imageToTextOverrides: PresetLiveModelOverrides = PresetLiveModelOverrides(),
    val textToTextOverrides: PresetLiveModelOverrides = PresetLiveModelOverrides(),
)

@Serializable
data class PresetRuntimeSettings(
    val providerSettings: PresetProviderSettings = GeneratedPresetModelCatalogData.providerSettings,
    val modelPriorityChains: PresetModelPriorityChains = GeneratedPresetModelCatalogData.modelPriorityChains,
    val adaptiveModelPriority: PresetAdaptiveModelPriority = PresetAdaptiveModelPriority(),
)
