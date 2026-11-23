package dev.screengoated.toolbox.mobile.preset.ui

import dev.screengoated.toolbox.mobile.shared.preset.Preset

internal data class PresetEditAttempt(val state: Preset, val saved: Boolean)

internal fun attemptPresetEdit(
    current: Preset,
    updated: Preset,
    save: (Preset, Preset) -> Boolean,
): PresetEditAttempt = if (save(current, updated)) {
    PresetEditAttempt(updated, true)
} else {
    PresetEditAttempt(current, false)
}
