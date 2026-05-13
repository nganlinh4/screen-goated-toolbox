package dev.screengoated.toolbox.mobile.downloader.ui

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal fun downloaderSettingsLabel(locale: MobileLocaleText): String = when (locale.closeLabel) {
    "Đóng" -> "Cài đặt"
    "닫기" -> "설정"
    else -> "Settings"
}

internal fun downloaderClearLabel(locale: MobileLocaleText): String = when (locale.closeLabel) {
    "Đóng" -> "Xóa"
    "닫기" -> "지우기"
    else -> "Clear"
}

internal fun downloaderPasteLabel(locale: MobileLocaleText): String = when (locale.closeLabel) {
    "Đóng" -> "Dán"
    "닫기" -> "붙여넣기"
    else -> "Paste"
}

internal fun downloaderNewTabLabel(locale: MobileLocaleText): String = when (locale.closeLabel) {
    "Đóng" -> "Tab mới"
    "닫기" -> "새 탭"
    else -> "New tab"
}

internal fun downloaderUnsupportedFolderText(locale: MobileLocaleText): String = when (locale.closeLabel) {
    "Đóng" -> "Chỉ hỗ trợ bộ nhớ trong cho tải xuống."
    "닫기" -> "다운로드는 내부 저장소 폴더만 지원합니다."
    else -> "Downloads only support internal storage folders."
}

internal fun downloaderOpenFileFailedText(locale: MobileLocaleText): String = when (locale.closeLabel) {
    "Đóng" -> "Không thể mở file đã tải."
    "닫기" -> "다운로드한 파일을 열 수 없습니다."
    else -> "Could not open the downloaded file."
}

internal fun downloaderOpenFolderFailedText(locale: MobileLocaleText): String = when (locale.closeLabel) {
    "Đóng" -> "Không thể mở thư mục tải xuống."
    "닫기" -> "다운로드 폴더를 열 수 없습니다."
    else -> "Could not open the download folder."
}
