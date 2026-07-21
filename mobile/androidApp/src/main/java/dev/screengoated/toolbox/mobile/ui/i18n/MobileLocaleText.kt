package dev.screengoated.toolbox.mobile.ui.i18n

import java.util.Locale
import kotlin.random.Random

/**
 * Localized text catalog for the mobile shell.
 *
 * The root owns only cohesive locale sections. Interfaces delegated by each section preserve
 * the original flat property API for consumers while keeping every generated JVM signature
 * comfortably below the 255-slot limit.
 */
data class MobileLocaleText(
    val localeCode: String,
    val shell: MobileShellLocale,
    val creationApps: MobileCreationAppsLocale,
    val history: MobileHistoryLocale,
    val help: MobileHelpLocale,
    val translationGummy: MobileTranslationGummyLocale,
    val providers: MobileProviderLocale,
    val presetRuntime: MobilePresetRuntimeLocale,
    val updates: MobileUpdateLocale,
    val customModels: MobileCustomModelsLocale,
    val appearance: MobileAppearanceLocale,
    val ttsSettings: MobileTtsSettingsLocale,
    val ttsVoice: MobileTtsVoiceLocale,
    val download: MobileDownloadLocale,
    val downloadOptions: MobileDownloadOptionsLocale,
    val downloader: MobileDownloaderLocale,
) : MobileShellText by shell,
    MobileCreationAppsText by creationApps,
    MobileHistoryText by history,
    MobileHelpText by help,
    MobileTranslationGummyText by translationGummy,
    MobileProviderText by providers,
    MobilePresetRuntimeText by presetRuntime,
    MobileUpdateText by updates,
    MobileCustomModelsText by customModels,
    MobileAppearanceText by appearance,
    MobileTtsSettingsText by ttsSettings,
    MobileTtsVoiceText by ttsVoice,
    MobileDownloadText by download,
    MobileDownloadOptionsText by downloadOptions {
    fun ttsFailedLoadVoices(error: String): String {
        return ttsSettings.ttsFailedLoadVoicesTemplate.replace("{}", error)
    }

    fun compactOverlayOpacityLabel(): String {
        return appearance.compactOverlayOpacityLabel
    }

    fun resetDefaultsDoneMessage(): String {
        return appearance.resetDefaultsDoneMessage
    }

    fun toolsHelpCopy(): MobileToolsHelpCopy {
        return help.toolsHelpCopy
    }

    fun languageCode(): String {
        return localeCode
    }

    fun nextPreviewText(
        voiceName: String,
        previousIndex: Int,
        random: Random = Random(System.currentTimeMillis()),
    ): PreviewSelection {
        val previewTexts = ttsVoice.previewTexts
        if (previewTexts.isEmpty()) {
            return PreviewSelection(index = -1, text = voiceName)
        }
        val nextIndex = if (previewTexts.size == 1) {
            0
        } else {
            val candidate = random.nextInt(previewTexts.size)
            if (candidate == previousIndex) {
                (candidate + 1) % previewTexts.size
            } else {
                candidate
            }
        }
        return PreviewSelection(
            index = nextIndex,
            text = previewTexts[nextIndex].replace("{}", voiceName),
        )
    }

    companion object {
        fun forLanguage(rawCode: String): MobileLocaleText {
            return when (rawCode.lowercase(Locale.US)) {
                "vi" -> vietnameseMobileLocaleText()
                "ko" -> koreanMobileLocaleText()
                else -> englishMobileLocaleText()
            }
        }
    }
}
