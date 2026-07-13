package dev.screengoated.toolbox.mobile.ui.i18n

interface OverlayChromeText {
    val placeholderText: String
    val copyTextTitle: String
    val decreaseFontTitle: String
    val increaseFontTitle: String
    val toggleTranscriptionTitle: String
    val toggleTranslationTitle: String
    val toggleHeaderTitle: String
    val micInputTitle: String
    val deviceAudioTitle: String
    val geminiLiveTitle: String
    val geminiS2sTitle: String
    val llmLabel: String
    val gtxLabel: String
    val transcriptionModelTitle: String
    val translationModelTitle: String
    val transcriptionLanguageTitle: String
    val unavailableSuffix: String
    val targetLanguageTitle: String
    val s2sTranslationModelTitle: String
    val s2sTargetLanguageTitle: String
}

interface OverlayControlText {
    val directSpeechTitle: String
    val ttsS2sLockedTitle: String
    val ttsEnableTitle: String
    val overlayOpacityLabel: String
    val ttsSettingsTitle: String
    val ttsTitle: String
    val ttsSpeed: String
    val ttsAuto: String
    val ttsVolume: String
    val downloadingModelTitle: String
    val pleaseWaitText: String
    val cancelText: String
    val pickerSearchHint: String
}

data class OverlayChromeLocale(
    override val placeholderText: String,
    override val copyTextTitle: String,
    override val decreaseFontTitle: String,
    override val increaseFontTitle: String,
    override val toggleTranscriptionTitle: String,
    override val toggleTranslationTitle: String,
    override val toggleHeaderTitle: String,
    override val micInputTitle: String,
    override val deviceAudioTitle: String,
    override val geminiLiveTitle: String,
    override val geminiS2sTitle: String,
    override val llmLabel: String,
    override val gtxLabel: String,
    override val transcriptionModelTitle: String,
    override val translationModelTitle: String,
    override val transcriptionLanguageTitle: String,
    override val unavailableSuffix: String,
    override val targetLanguageTitle: String,
    override val s2sTranslationModelTitle: String,
    override val s2sTargetLanguageTitle: String,
) : OverlayChromeText

data class OverlayControlLocale(
    override val directSpeechTitle: String,
    override val ttsS2sLockedTitle: String,
    override val ttsEnableTitle: String,
    override val overlayOpacityLabel: String,
    override val ttsSettingsTitle: String,
    override val ttsTitle: String,
    override val ttsSpeed: String,
    override val ttsAuto: String,
    override val ttsVolume: String,
    override val downloadingModelTitle: String,
    override val pleaseWaitText: String,
    override val cancelText: String,
    override val pickerSearchHint: String,
) : OverlayControlText

data class OverlayLocaleText(
    val chrome: OverlayChromeLocale,
    val controls: OverlayControlLocale,
) : OverlayChromeText by chrome,
    OverlayControlText by controls
