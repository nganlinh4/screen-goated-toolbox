package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

enum class PresetPlaceholderReason {
    IMAGE_CAPTURE_NOT_READY,
    TEXT_SELECTION_NOT_READY,
    TEXT_INPUT_OVERLAY_NOT_READY,
    MODEL_PROVIDER_NOT_READY,
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
)

data class PresetCatalogState(
    val presets: List<ResolvedPreset> = emptyList(),
) {
    fun findPreset(id: String): ResolvedPreset? = presets.firstOrNull { it.preset.id == id }

    fun presetsFor(type: PresetType): List<ResolvedPreset> =
        presets.filter { it.preset.presetType == type }
}
