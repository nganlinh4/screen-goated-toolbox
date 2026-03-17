package dev.screengoated.toolbox.mobile.preset

import kotlinx.serialization.Serializable

@Serializable
data class PresetProviderSettings(
    val useGroq: Boolean = GeneratedPresetModelCatalogData.providerSettings.useGroq,
    val useGemini: Boolean = GeneratedPresetModelCatalogData.providerSettings.useGemini,
    val useOpenRouter: Boolean = GeneratedPresetModelCatalogData.providerSettings.useOpenRouter,
    val useCerebras: Boolean = GeneratedPresetModelCatalogData.providerSettings.useCerebras,
    val useOllama: Boolean = GeneratedPresetModelCatalogData.providerSettings.useOllama,
)

@Serializable
data class PresetModelPriorityChains(
    val imageToText: List<String> = GeneratedPresetModelCatalogData.modelPriorityChains.imageToText,
    val textToText: List<String> = GeneratedPresetModelCatalogData.modelPriorityChains.textToText,
)

@Serializable
data class PresetRuntimeSettings(
    val providerSettings: PresetProviderSettings = GeneratedPresetModelCatalogData.providerSettings,
    val modelPriorityChains: PresetModelPriorityChains = GeneratedPresetModelCatalogData.modelPriorityChains,
)
