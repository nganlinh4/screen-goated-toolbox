package dev.screengoated.toolbox.mobile.ui.i18n

/**
 * Shared value types for the mobile locale text catalog.
 *
 * These are intentionally kept separate from [MobileLocaleText] and the per-language
 * factories so the catalog can grow without any single file becoming unwieldy.
 */

data class MobileUiLanguageOption(
    val code: String,
    val label: String,
)

data class OverlayLocaleText(
    val placeholderText: String,
    val copyTextTitle: String,
    val decreaseFontTitle: String,
    val increaseFontTitle: String,
    val toggleTranscriptionTitle: String,
    val toggleTranslationTitle: String,
    val toggleHeaderTitle: String,
    val micInputTitle: String,
    val deviceAudioTitle: String,
    val geminiLiveTitle: String,
    val geminiS2sTitle: String,
    val llmLabel: String,
    val gtxLabel: String,
    val transcriptionModelTitle: String,
    val translationModelTitle: String,
    val transcriptionLanguageTitle: String,
    val unavailableSuffix: String,
    val targetLanguageTitle: String,
    val s2sTranslationModelTitle: String,
    val s2sTargetLanguageTitle: String,
    val directSpeechTitle: String,
    val ttsS2sLockedTitle: String,
    val ttsEnableTitle: String,
    val overlayOpacityLabel: String,
    val ttsSettingsTitle: String,
    val ttsTitle: String,
    val ttsSpeed: String,
    val ttsAuto: String,
    val ttsVolume: String,
    val downloadingModelTitle: String,
    val pleaseWaitText: String,
    val cancelText: String,
    val pickerSearchHint: String,
)

data class MobileToolsHelpCopy(
    val title: String,
    val bubbleTitle: String,
    val bubbleDescription: String,
    val favoriteTitle: String,
    val favoriteDescription: String,
    val dismiss: String,
)

data class PreviewSelection(
    val index: Int,
    val text: String,
)
