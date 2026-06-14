package dev.screengoated.toolbox.mobile.ui.i18n

/**
 * Localized text for the downloader and downloaded-tools surfaces.
 *
 * Extracted from [MobileLocaleText] as a single cohesive group so the top-level catalog
 * constructor stays well under the JVM 254-parameter limit. Held on [MobileLocaleText] as
 * a single `downloader` field; consume via `locale.downloader.<field>`.
 */
data class MobileDownloaderLocale(
    // Downloaded tools UI
    val toolDescYtdlp: String,
    val toolDescFfmpeg: String,
    val toolRuntimeMoonshine: String,
    val toolRuntimeMoonshineDesc: String,
    val toolRuntimeOrt: String,
    val toolRuntimeOrtDesc: String,
    val toolRuntimeSherpa: String,
    val toolRuntimeSherpaDesc: String,
    val toolUpdate: String,
    val toolDelete: String,
    val toolUpdated: String,
    val toolUpToDate: String,
    val toolUpdating: String,
    val toolUpdateFailed: String,
    // Downloader toolbar / status strings (previously selected by matching closeLabel)
    val downloaderSettings: String,
    val downloaderClear: String,
    val downloaderPaste: String,
    val downloaderNewTab: String,
    val downloaderUnsupportedFolder: String,
    val downloaderOpenFileFailed: String,
    val downloaderOpenFolderFailed: String,
    val downloaderStartNowPreferTemplate: String,
    val downloaderCopyVideo: String,
    val downloaderCopyVideoDone: String,
    val downloaderCopyVideoFailed: String,
    // Downloaded tools dialog summaries / help (previously selected by matching closeLabel)
    val moonshineLanguageSummary: String,
    val zipformerLanguageSummary: String,
    val moonshineHelpText: String,
    val zipformerHelpText: String,
) {
    fun downloaderStartNowPrefer(quality: String): String {
        return downloaderStartNowPreferTemplate.replace("{}", quality)
    }
}
