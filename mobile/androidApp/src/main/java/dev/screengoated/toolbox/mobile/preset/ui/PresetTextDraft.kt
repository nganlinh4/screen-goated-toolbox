package dev.screengoated.toolbox.mobile.preset.ui

internal data class PresetTextDraft(val text: String, val isOpen: Boolean)

internal fun savePresetTextDraft(
    draft: PresetTextDraft,
    save: (String) -> Boolean,
): PresetTextDraft = if (save(draft.text)) draft.copy(isOpen = false) else draft
