package dev.screengoated.toolbox.mobile.preset

internal data class AdaptiveCandidateRank(
    val qualityTier: Int,
    val latencyMs: Int,
) {
    fun priorityCost(): Long {
        val distance = 6 - qualityTier.coerceIn(1, 6)
        var numerator = latencyMs.coerceAtLeast(0).toLong()
        repeat(distance) { numerator = (numerator * 3L).coerceAtMost(Long.MAX_VALUE / 2L) }
        return numerator / (1L shl distance)
    }

    fun outranksOrTies(other: AdaptiveCandidateRank): Boolean =
        priorityCost() < other.priorityCost() ||
            priorityCost() == other.priorityCost() &&
            (qualityTier > other.qualityTier ||
                qualityTier == other.qualityTier && latencyMs <= other.latencyMs)
}

internal sealed interface AdaptiveManualEdit {
    data class Replace(val old: String, val new: String) : AdaptiveManualEdit
    data class Remove(val id: String) : AdaptiveManualEdit
    data class Move(val id: String) : AdaptiveManualEdit
    data class Add(val id: String) : AdaptiveManualEdit
}

internal data class AdaptiveEditResult(
    val authored: List<String>,
    val overrides: PresetLiveModelOverrides,
    val remainsEnabled: Boolean,
)

internal fun PresetRetryChainKind.effectiveChain(
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
): List<String> {
    val configured = configuredChain(settings)
    val adaptive = settings.adaptiveModelPriority
    val enabled = when (this) {
        PresetRetryChainKind.IMAGE_TO_TEXT -> adaptive.imageToText
        PresetRetryChainKind.TEXT_TO_TEXT -> adaptive.textToText
    }
    if (!enabled) return configured
    val overrides = when (this) {
        PresetRetryChainKind.IMAGE_TO_TEXT -> adaptive.imageToTextOverrides
        PresetRetryChainKind.TEXT_TO_TEXT -> adaptive.textToTextOverrides
    }
    return adaptiveChain(configured, overrides, settings, apiKeys)
}

internal fun PresetRetryChainKind.adaptiveChain(
    configured: List<String>,
    overrides: PresetLiveModelOverrides,
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
): List<String> {
    val offered = offeredModels(settings, apiKeys, targetModelType())
    val reconciled = reconcileConfiguredModels(
        configured,
        settings,
        apiKeys,
        targetModelType(),
        offered.map(Pair<String, Int>::first).toSet(),
    )
    if (offered.isEmpty()) return reconciled
    val latencyById = offered.toMap()
    return mergeAdaptiveModels(
        configured = reconciled,
        offered = offered.map(Pair<String, Int>::first),
        pinned = overrides.pinned,
        excluded = overrides.excluded,
    ) { id ->
        val model = PresetModelCatalog.getById(id)
        AdaptiveCandidateRank(
            qualityTier = model?.intelligenceTier ?: 4,
            latencyMs = latencyById[id] ?: model?.typicalLatencyMs ?: Int.MAX_VALUE,
        )
    }
}

private fun reconcileConfiguredModels(
    configured: List<String>,
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
    wantedType: PresetModelType,
    offered: Set<String>,
): List<String> {
    if (!settings.providerSettings.useNvidia || apiKeys.nvidiaKey.isBlank()) return configured
    val feed = PresetModelFeed.current() ?: return configured
    return configured.filter { id ->
        val model = PresetModelCatalog.getById(id)
        model == null || model.provider != PresetModelProvider.NVIDIA ||
            model.modelType != wantedType || id in offered
    }
}

internal fun offeredModels(
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
    wantedType: PresetModelType,
): List<Pair<String, Int>> {
    if (!settings.providerSettings.useNvidia || apiKeys.nvidiaKey.isBlank()) return emptyList()
    val feed = PresetModelFeed.current() ?: return emptyList()
    return rankedFeedModels(feed)
        .filter { feedModelType(it) == wantedType }
        .mapNotNull { model ->
            resolveFeedEndpoint(feed.provider, model.endpoint, wantedType)
                ?.let { it to (model.p50Ms ?: Int.MAX_VALUE) }
        }
}

internal fun feedAllowsRuntimeModel(
    model: PresetModelDescriptor,
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
): Boolean {
    if (model.provider != PresetModelProvider.NVIDIA ||
        !settings.providerSettings.useNvidia || apiKeys.nvidiaKey.isBlank()
    ) return true
    if ("nvidia:${model.fullName}" in GeneratedPresetModelCatalogData.withdrawnEndpoints) return false
    val feed = PresetModelFeed.current() ?: return true
    return rankedFeedModels(feed).any {
        it.endpoint == model.fullName && feedModelType(it) == model.modelType
    }
}

internal fun mergeAdaptiveModels(
    configured: List<String>,
    offered: List<String>,
    pinned: List<String>,
    excluded: List<String>,
    rankFor: (String) -> AdaptiveCandidateRank,
): List<String> {
    if (configured.isEmpty()) return configured
    val protectedHead = configured.first()
    fun isPinned(id: String) = id in pinned && id in configured
    val adaptive = offered
        .asSequence()
        .filter { it != protectedHead && !isPinned(it) && it !in excluded }
        .distinct()
        .sortedWith(
            compareBy<String> { rankFor(it).priorityCost() }
                .thenByDescending { rankFor(it).qualityTier }
                .thenBy { rankFor(it).latencyMs },
        )
        .take(MAXIMUM_ADAPTIVE_OFFERS)
        .toList()
    val merged = configured.filterIndexed { index, id ->
        index == 0 || id !in excluded && (isPinned(id) || id !in offered)
    }.toMutableList()
    if (merged.size >= PROTECTED_LOCAL_LEADERS) {
        adaptive.forEach { id ->
            if (id in merged) return@forEach
            val candidateRank = rankFor(id)
            val index = merged.indices.drop(PROTECTED_LOCAL_LEADERS).firstOrNull {
                !rankFor(merged[it]).outranksOrTies(candidateRank)
            } ?: merged.size
            merged.add(index, id)
        }
    }
    configured.withIndex().drop(1).filter { isPinned(it.value) && it.value !in excluded }
        .forEach { (authoredIndex, id) ->
            val current = merged.indexOf(id)
            if (current >= 0) {
                merged.removeAt(current)
                merged.add(minOf(authoredIndex, merged.size), id)
            }
        }
    return merged
}

internal fun commitAdaptiveEdits(
    visible: List<String>,
    currentOverrides: PresetLiveModelOverrides,
    liveIds: List<String>,
    edits: List<AdaptiveManualEdit>,
): AdaptiveEditResult {
    val pinned = currentOverrides.pinned.distinct().toMutableList()
    val excluded = currentOverrides.excluded.distinct().toMutableList()
    fun isLiveOwned(id: String) = id in liveIds || id in pinned || id in excluded
    fun pin(id: String) {
        excluded.remove(id)
        if (id !in pinned) pinned.add(id)
    }
    fun exclude(id: String) {
        pinned.remove(id)
        if (id !in excluded) excluded.add(id)
    }
    edits.forEach { edit ->
        when (edit) {
            is AdaptiveManualEdit.Replace -> {
                if (isLiveOwned(edit.old)) exclude(edit.old)
                if (isLiveOwned(edit.new)) pin(edit.new)
            }
            is AdaptiveManualEdit.Remove -> if (isLiveOwned(edit.id)) exclude(edit.id)
            is AdaptiveManualEdit.Move -> if (isLiveOwned(edit.id)) pin(edit.id)
            is AdaptiveManualEdit.Add -> if (isLiveOwned(edit.id)) pin(edit.id)
        }
    }
    val remainsEnabled = visible.any { it in liveIds || it in pinned }
    return AdaptiveEditResult(
        authored = visible,
        overrides = PresetLiveModelOverrides(
            pinned = pinned.filterNot { it in excluded },
            excluded = excluded,
        ),
        remainsEnabled = remainsEnabled,
    )
}

private fun resolveFeedEndpoint(
    provider: String,
    endpoint: String,
    modelType: PresetModelType,
): String? {
    if ("$provider:$endpoint" in GeneratedPresetModelCatalogData.withdrawnEndpoints) return null
    val known = GeneratedPresetModelCatalogData.knownEndpoints.firstOrNull {
        it.provider == PresetModelProvider.NVIDIA &&
            it.fullName == endpoint &&
            it.modelType == modelType
    }
    if (known != null) {
        return PresetModelCatalog.builtInForEndpoint(
            known.provider,
            known.fullName,
            known.modelType,
        )?.id
    }
    return discoveredModelId(provider, endpoint)
}

private const val MAXIMUM_ADAPTIVE_OFFERS = 5
private const val PROTECTED_LOCAL_LEADERS = 2
