package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.content.res.Configuration
import dev.screengoated.toolbox.mobile.model.MobileThemeMode

internal fun overlayLocalized(
    uiLanguage: String,
    en: String,
    vi: String,
    ko: String,
): String = when (uiLanguage) {
    "vi" -> vi
    "ko" -> ko
    else -> en
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
