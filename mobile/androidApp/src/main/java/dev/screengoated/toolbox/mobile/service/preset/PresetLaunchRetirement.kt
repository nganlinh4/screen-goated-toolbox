package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.ResolvedPreset

/** A rejected launch is still a replacement attempt, unlike a panel information request. */
internal fun retireUnsupportedPresetLaunch(resolved: ResolvedPreset, retire: () -> Unit): Boolean {
    if (resolved.executionCapability.supported) return false
    retire()
    return true
}
