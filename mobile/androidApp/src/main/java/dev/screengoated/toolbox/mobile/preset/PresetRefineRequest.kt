package dev.screengoated.toolbox.mobile.preset

internal data class PresetRefineResult(
    val text: String,
    val modelId: String,
    val modelProvider: PresetModelProvider,
)

internal data class PresetRefineTextRequest(
    val prompt: String,
    val inputText: String,
)

internal fun initialRefineModel(
    originalModelId: String,
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
): PresetModelDescriptor? {
    val original = PresetModelCatalog.getById(originalModelId) ?: return null
    original
        .takeIf { it.modelType == PresetModelType.TEXT && !it.isNonLlm }
        ?.let { return it }
    return PresetRetryChainKind.TEXT_TO_TEXT.effectiveChain(settings, apiKeys)
        .firstNotNullOfOrNull { id ->
            PresetModelCatalog.getById(id)?.takeIf {
                it.modelType == PresetModelType.TEXT && !it.isNonLlm
            }
        }
}

/**
 * Mirrors Windows `refine` (src/api/text/refine/mod.rs): the previous result is
 * embedded once in the final prompt and no separate input text is sent.
 */
internal fun presetRefineTextRequest(previousText: String, refinePrompt: String) = PresetRefineTextRequest(
    prompt = "Content:\n$previousText\n\nInstruction:\n$refinePrompt\n\nOutput ONLY the result.",
    inputText = "",
)
