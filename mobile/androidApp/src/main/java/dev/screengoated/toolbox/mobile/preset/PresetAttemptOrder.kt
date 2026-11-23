package dev.screengoated.toolbox.mobile.preset

internal const val CHOSEN_MODEL_ROW = "@chosen-model"

internal fun PresetRetryChainKind.chosenAfter(settings: PresetRuntimeSettings): Int = when (this) {
    PresetRetryChainKind.IMAGE_TO_TEXT -> settings.modelPriorityChains.imageChosenAfter
    PresetRetryChainKind.TEXT_TO_TEXT -> settings.modelPriorityChains.textChosenAfter
}

internal fun editablePriorityOrder(rows: List<String>, after: Int): List<String> =
    rows.toMutableList().apply { add(after.takeIf { it in 0..size } ?: 0, CHOSEN_MODEL_ROW) }

internal fun splitPriorityOrder(rows: List<String>): Pair<List<String>, Int> =
    rows.filterNot { it == CHOSEN_MODEL_ROW } to rows.indexOf(CHOSEN_MODEL_ROW).coerceAtLeast(0)

internal fun normalizedChosenAfter(rows: List<String>, after: Int): Int =
    if (after in 0..rows.size) rows.take(after).filter(String::isNotBlank).distinct().size else 0

internal fun expandChosenModel(rows: List<String>, selected: String): List<String> =
    (if (CHOSEN_MODEL_ROW in rows) rows else editablePriorityOrder(rows, 0)).mapNotNull { row ->
        when (row) {
            CHOSEN_MODEL_ROW -> selected
            selected -> null
            else -> row
        }
    }.distinct()

internal class PresetAttemptOrder(
    private val kind: PresetRetryChainKind,
    private val selected: String,
    private val searchEnabled: Boolean,
    private val keys: ApiKeys,
    private val settings: PresetRuntimeSettings,
) {
    private val ordered = if (kind.chosenAfter(settings) in 1..kind.configuredChain(settings).size) {
        expandChosenModel(kind.effectiveChain(settings, keys, includeChosenModel = true), selected)
    } else {
        null
    }

    fun initialId(): String {
        val rows = ordered ?: return selected
        val chosen = PresetModelCatalog.getById(selected) ?: error("Unknown model config: $selected")
        require(chosen.modelType == kind.targetModelType()) { "Incompatible model: $selected" }
        requireExplicitSearchOptIn(chosen, searchEnabled)
        return resolveNextConfiguredRetryModel(
            selected, emptyList(), emptySet(), kind, keys, settings, searchEnabled,
            rows.takeWhile { it != selected },
        )?.id ?: selected
    }

    fun next(
        current: String,
        failed: List<String>,
        blocked: Set<PresetModelProvider>,
    ): PresetModelDescriptor? = resolveNextRetryModel(
        current, failed, blocked, kind, keys, settings, searchEnabled, ordered,
    )
}
