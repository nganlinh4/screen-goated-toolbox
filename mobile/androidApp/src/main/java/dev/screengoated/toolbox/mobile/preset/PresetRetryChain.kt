package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockType

internal enum class PresetRetryChainKind {
    IMAGE_TO_TEXT,
    TEXT_TO_TEXT,
}

internal fun PresetRetryChainKind.targetModelType(): PresetModelType = when (this) {
    PresetRetryChainKind.IMAGE_TO_TEXT -> PresetModelType.VISION
    PresetRetryChainKind.TEXT_TO_TEXT -> PresetModelType.TEXT
}

internal fun PresetRetryChainKind.configuredChain(
    settings: PresetRuntimeSettings,
): List<String> = when (this) {
    PresetRetryChainKind.IMAGE_TO_TEXT -> settings.modelPriorityChains.imageToText
    PresetRetryChainKind.TEXT_TO_TEXT -> settings.modelPriorityChains.textToText
}

internal fun retryChainKindForBlockType(blockType: BlockType): PresetRetryChainKind? = when (blockType) {
    BlockType.IMAGE -> PresetRetryChainKind.IMAGE_TO_TEXT
    BlockType.TEXT -> PresetRetryChainKind.TEXT_TO_TEXT
    else -> null
}

internal fun providerIsAvailable(
    provider: PresetModelProvider,
    apiKeys: ApiKeys,
    settings: PresetRuntimeSettings,
): Boolean = when (provider) {
    PresetModelProvider.GROQ ->
        settings.providerSettings.useGroq && apiKeys.groqKey.isNotBlank()
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GEMINI_LIVE,
    -> settings.providerSettings.useGemini && apiKeys.geminiKey.isNotBlank()
    PresetModelProvider.OPENROUTER ->
        settings.providerSettings.useOpenRouter && apiKeys.openRouterKey.isNotBlank()
    PresetModelProvider.CEREBRAS ->
        settings.providerSettings.useCerebras && apiKeys.cerebrasKey.isNotBlank()
    PresetModelProvider.OLLAMA ->
        settings.providerSettings.useOllama && apiKeys.ollamaBaseUrl.isNotBlank()
    PresetModelProvider.GOOGLE_GTX,
    PresetModelProvider.QRSERVER,
    PresetModelProvider.PARAKEET,
    -> true
}

internal fun preflightSkipReason(
    modelId: String,
    provider: PresetModelProvider,
    apiKeys: ApiKeys,
    blockedProviders: Set<PresetModelProvider>,
    settings: PresetRuntimeSettings,
): String? {
    if (provider in blockedProviders) {
        return "Provider ${providerKey(provider)} is unavailable for retry."
    }
    if (!providerIsAvailable(provider, apiKeys, settings)) {
        return when (provider) {
            PresetModelProvider.GROQ ->
                if (!settings.providerSettings.useGroq) "PROVIDER_DISABLED:groq" else "NO_API_KEY:groq"
            PresetModelProvider.GOOGLE,
            PresetModelProvider.GEMINI_LIVE,
            -> {
                val providerName = providerKey(provider)
                if (!settings.providerSettings.useGemini) {
                    "PROVIDER_DISABLED:$providerName"
                } else {
                    "NO_API_KEY:$providerName"
                }
            }
            PresetModelProvider.OPENROUTER ->
                if (!settings.providerSettings.useOpenRouter) "PROVIDER_DISABLED:openrouter" else "NO_API_KEY:openrouter"
            PresetModelProvider.CEREBRAS ->
                if (!settings.providerSettings.useCerebras) "PROVIDER_DISABLED:cerebras" else "NO_API_KEY:cerebras"
            PresetModelProvider.OLLAMA ->
                if (!settings.providerSettings.useOllama) "PROVIDER_DISABLED:ollama" else "OLLAMA_URL_MISSING"
            else -> null
        }
    }
    if (PresetModelCatalog.getById(modelId) == null) {
        return "Model config not found: $modelId"
    }
    return null
}

internal fun shouldAdvanceRetryChain(error: String): Boolean {
    if (
        error.contains("NO_API_KEY") ||
        error.contains("INVALID_API_KEY") ||
        error.contains("PROVIDER_NOT_READY")
    ) {
        return true
    }
    extractHttpStatusCode(error)?.let { code ->
        return when (code) {
            400, 401, 403, 404, 429 -> true
            in 500..599 -> true
            else -> false
        }
    }
    val lower = error.lowercase()
    return lower.contains("rate limit") ||
        lower.contains("too many requests") ||
        lower.contains("quota exceeded") ||
        lower.contains("peer disconnected") ||
        lower.contains("connection reset") ||
        lower.contains("connection aborted") ||
        lower.contains("broken pipe") ||
        lower.contains("timed out") ||
        lower.contains("timeout") ||
        lower.contains("not found") ||
        lower.contains("unsupported") ||
        lower.contains("not support")
}

internal fun shouldBlockRetryProvider(error: String): Boolean {
    if (
        error.contains("NO_API_KEY") ||
        error.contains("INVALID_API_KEY") ||
        error.contains("PROVIDER_DISABLED") ||
        error.contains("PROVIDER_NOT_READY")
    ) {
        return true
    }
    return extractHttpStatusCode(error) in setOf(401, 403)
}

internal fun resolveNextRetryModel(
    currentModelId: String,
    failedModelIds: List<String>,
    blockedProviders: Set<PresetModelProvider>,
    chainKind: PresetRetryChainKind,
    apiKeys: ApiKeys,
    settings: PresetRuntimeSettings,
): PresetModelDescriptor? {
    val current = PresetModelCatalog.getById(currentModelId) ?: return null
    val mustSupportSearch = PresetModelCatalog.supportsSearchById(currentModelId)
    val targetType = chainKind.targetModelType()
    val explicit = chainKind.configuredChain(settings)

    explicit.firstNotNullOfOrNull { candidateId ->
        val candidate = PresetModelCatalog.getById(candidateId) ?: return@firstNotNullOfOrNull null
        if (candidate.id == currentModelId || candidate.id in failedModelIds) {
            return@firstNotNullOfOrNull null
        }
        if (isRetryCandidateCompatible(candidate, targetType, mustSupportSearch, blockedProviders, apiKeys, settings)) {
            candidate
        } else {
            null
        }
    }?.let { return it }

    PresetModelCatalog.models
        .filter { it.provider == current.provider }
        .lastOrNull { candidate ->
            candidate.id != currentModelId &&
                candidate.id !in failedModelIds &&
                isRetryCandidateCompatible(candidate, targetType, mustSupportSearch, blockedProviders, apiKeys, settings)
        }?.let { return it }

    return PresetModelCatalog.models
        .filter { it.provider != current.provider }
        .lastOrNull { candidate ->
            candidate.id !in failedModelIds &&
                isRetryCandidateCompatible(candidate, targetType, mustSupportSearch, blockedProviders, apiKeys, settings)
        }
}

private fun isRetryCandidateCompatible(
    model: PresetModelDescriptor,
    targetType: PresetModelType,
    mustSupportSearch: Boolean,
    blockedProviders: Set<PresetModelProvider>,
    apiKeys: ApiKeys,
    settings: PresetRuntimeSettings,
): Boolean {
    return model.modelType == targetType &&
        !model.isNonLlm &&
        model.provider !in blockedProviders &&
        providerIsAvailable(model.provider, apiKeys, settings) &&
        (!mustSupportSearch || PresetModelCatalog.supportsSearchByName(model.fullName))
}

private fun providerKey(provider: PresetModelProvider): String = when (provider) {
    PresetModelProvider.GOOGLE -> "google"
    PresetModelProvider.CEREBRAS -> "cerebras"
    PresetModelProvider.GROQ -> "groq"
    PresetModelProvider.OPENROUTER -> "openrouter"
    PresetModelProvider.GOOGLE_GTX -> "google-gtx"
    PresetModelProvider.GEMINI_LIVE -> "gemini-live"
    PresetModelProvider.OLLAMA -> "ollama"
    PresetModelProvider.QRSERVER -> "qrserver"
    PresetModelProvider.PARAKEET -> "parakeet"
}

private fun extractHttpStatusCode(error: String): Int? {
    val regex = Regex("""\b(4\d{2}|5\d{2})\b""")
    return regex.find(error)?.value?.toIntOrNull()
}
