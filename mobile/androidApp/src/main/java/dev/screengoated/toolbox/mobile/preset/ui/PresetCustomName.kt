package dev.screengoated.toolbox.mobile.preset.ui

import dev.screengoated.toolbox.mobile.shared.preset.Preset

/**
 * A custom preset has one user-chosen name (Windows stores a single name), so a
 * rename must be visible in every UI language, as create and duplicate already are.
 */
internal fun Preset.withCustomName(name: String): Preset =
    copy(nameEn = name, nameVi = name, nameKo = name)
