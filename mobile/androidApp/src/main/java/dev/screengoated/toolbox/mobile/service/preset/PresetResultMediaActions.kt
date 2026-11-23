package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState

internal const val MAX_RESULT_CLIPBOARD_UTF16_UNITS = 262_144

internal enum class PresetResultMediaKind { NONE, IMAGE, AUDIO }

internal fun resultMediaKind(state: PresetResultWindowState): PresetResultMediaKind = when {
    state.inputImageHtml?.get() === state.markdownText -> PresetResultMediaKind.IMAGE
    state.inputAudioHtml?.get() === state.markdownText -> PresetResultMediaKind.AUDIO
    else -> PresetResultMediaKind.NONE
}

internal fun resultDisabledActions(state: PresetResultWindowState): Set<String> = when (resultMediaKind(state)) {
    PresetResultMediaKind.IMAGE -> setOf("speaker", "edit", "download")
    PresetResultMediaKind.AUDIO -> setOf("copy", "speaker", "edit")
    PresetResultMediaKind.NONE -> emptySet()
}

internal fun inputImageForCopy(state: PresetResultWindowState): ByteArray? =
    state.inputImageBytes?.takeIf { resultMediaKind(state) == PresetResultMediaKind.IMAGE }

internal fun resultTextFitsClipboard(text: String): Boolean = text.length <= MAX_RESULT_CLIPBOARD_UTF16_UNITS

internal fun copyResultPayload(
    state: PresetResultWindowState,
    copyText: (String) -> Boolean,
    copyImage: (ByteArray) -> Boolean,
): Boolean = runCatching {
    when (resultMediaKind(state)) {
        PresetResultMediaKind.IMAGE -> inputImageForCopy(state)?.let(copyImage) == true
        PresetResultMediaKind.AUDIO -> false
        PresetResultMediaKind.NONE -> resultTextFitsClipboard(state.markdownText) && copyText(state.markdownText)
    }
}.getOrDefault(false)
