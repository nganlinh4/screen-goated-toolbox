package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset

enum class PresetPlaceholderReason {
    IMAGE_CAPTURE_NOT_READY,
    TEXT_SELECTION_NOT_READY,
    TEXT_INPUT_OVERLAY_NOT_READY,
    MODEL_PROVIDER_NOT_READY,
    MODEL_UNAVAILABLE,
    AUDIO_CAPTURE_NOT_READY,
    REALTIME_AUDIO_NOT_READY,
    HTML_RESULT_NOT_READY,
    CONTROLLER_MODE_NOT_READY,
    AUTO_PASTE_NOT_READY,
    HOTKEYS_NOT_READY,
    GRAPH_EDITING_NOT_READY,
    NON_TEXT_GRAPH_NOT_READY,
}

data class PresetExecutionCapability(
    val supported: Boolean,
    val reason: PresetPlaceholderReason? = null,
)

data class ResolvedPreset(
    val preset: Preset,
    val hasOverride: Boolean,
    val isBuiltIn: Boolean,
    val executionCapability: PresetExecutionCapability,
    val placeholderReasons: Set<PresetPlaceholderReason>,
    val rowKey: String = if (isBuiltIn) builtInPresetRowKey(preset.id) else customPresetRowKey(preset.id),
    val rowIncarnation: Long = 0,
    val rowEpoch: String = "",
) {
    internal val rowIdentity: String get() = "$rowEpoch:$rowIncarnation:$rowKey"
}

internal fun builtInPresetRowKey(id: String): String = "built-in:$id"
internal fun customPresetRowKey(storageKey: String): String = "custom:$storageKey"

enum class PresetPersistenceFailure { READ, WRITE }

data class PresetCatalogState(
    val presets: List<ResolvedPreset> = emptyList(),
    val persistenceFailure: PresetPersistenceFailure? = null,
) {
    fun findPreset(id: String): ResolvedPreset? = presets.filter { it.preset.id == id }.singleOrNull()
    internal fun findRow(rowIdentity: String): ResolvedPreset? = presets.singleOrNull { it.rowIdentity == rowIdentity }
}
