package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.content.res.Configuration
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.preset.triLang

internal fun overlayLocalized(
    uiLanguage: String,
    en: String,
    vi: String,
    ko: String,
): String = triLang(uiLanguage, en, vi, ko)

/** One copy toast for manual and auto-copy of any block type, so it never names the content. */
internal fun copyToastText(uiLanguage: String, copied: Boolean): String =
    if (copied) {
        overlayLocalized(uiLanguage, "Copied", "Đã sao chép", "복사됨")
    } else {
        overlayLocalized(uiLanguage, "Could not copy result.", "Không thể sao chép kết quả.", "결과를 복사할 수 없습니다.")
    }

internal fun overlayIsDarkTheme(
    context: Context,
    themeMode: MobileThemeMode,
): Boolean {
    return when (themeMode) {
        MobileThemeMode.DARK -> true
        MobileThemeMode.LIGHT -> false
        MobileThemeMode.SYSTEM -> {
            val nightModeFlags = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
            nightModeFlags == Configuration.UI_MODE_NIGHT_YES
        }
    }
}
