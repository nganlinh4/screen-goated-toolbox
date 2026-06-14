package dev.screengoated.toolbox.mobile.downloader.ui

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal fun downloaderSettingsLabel(locale: MobileLocaleText): String = locale.downloader.downloaderSettings

internal fun downloaderClearLabel(locale: MobileLocaleText): String = locale.downloader.downloaderClear

internal fun downloaderPasteLabel(locale: MobileLocaleText): String = locale.downloader.downloaderPaste

internal fun downloaderNewTabLabel(locale: MobileLocaleText): String = locale.downloader.downloaderNewTab

internal fun downloaderUnsupportedFolderText(locale: MobileLocaleText): String =
    locale.downloader.downloaderUnsupportedFolder

internal fun downloaderOpenFileFailedText(locale: MobileLocaleText): String =
    locale.downloader.downloaderOpenFileFailed

internal fun downloaderOpenFolderFailedText(locale: MobileLocaleText): String =
    locale.downloader.downloaderOpenFolderFailed

internal fun downloaderStartNowText(locale: MobileLocaleText, quality: String?): String {
    if (quality.isNullOrBlank()) return locale.dlStartNowBtn
    return locale.downloader.downloaderStartNowPrefer(quality)
}

internal fun downloaderCopyVideoLabel(locale: MobileLocaleText): String = locale.downloader.downloaderCopyVideo

internal fun downloaderCopyVideoDoneText(locale: MobileLocaleText): String =
    locale.downloader.downloaderCopyVideoDone

internal fun downloaderCopyVideoFailedText(locale: MobileLocaleText): String =
    locale.downloader.downloaderCopyVideoFailed
