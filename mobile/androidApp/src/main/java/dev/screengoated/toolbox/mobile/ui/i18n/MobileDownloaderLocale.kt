package dev.screengoated.toolbox.mobile.ui.i18n

/**
 * Localized text for the downloader and downloaded-tools surfaces.
 *
 * Owned by [MobileLocaleText] as one cohesive section. Its constructor stays below the locale
 * section budget, and callers consume these fields through `locale.downloader.<field>`.
 */
data class MobileDownloaderLocale(
    // Downloaded tools UI
    val toolDescYtdlp: String,
    val toolDescFfmpeg: String,
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
    val preferQualityTemplate: String,
    val scanFormats: String,
    val downloaderCopyVideo: String,
    val downloaderCopyVideoDone: String,
    val downloaderCopyVideoFailed: String,
    // Downloaded tools: one-click cleanup of files left by removed offline engines
    val legacyOfflineModels: String,
    val legacyOfflineModelsDesc: String,
    val legacyOfflineModelsRemove: String,
    val legacyOfflineModelsRemoveFailed: String,
) {
    fun downloaderStartNowPrefer(quality: String): String {
        return downloaderStartNowPreferTemplate.replace("{}", quality)
    }

    fun preferQuality(quality: String): String = preferQualityTemplate.replace("{}", quality)
}
