package dev.screengoated.toolbox.mobile.model

import kotlinx.serialization.Serializable
import java.util.Locale

@Serializable
enum class MobileThemeMode {
    SYSTEM,
    DARK,
    LIGHT,
}

@Serializable
data class MobileUiPreferences(
    val themeMode: MobileThemeMode = MobileThemeMode.SYSTEM,
    val uiLanguage: String = defaultMobileUiLanguage(),
    val overlayOpacityPercent: Int = 85,
)

fun MobileThemeMode.next(): MobileThemeMode {
    return when (this) {
        MobileThemeMode.SYSTEM -> MobileThemeMode.DARK
        MobileThemeMode.DARK -> MobileThemeMode.LIGHT
        MobileThemeMode.LIGHT -> MobileThemeMode.SYSTEM
    }
}

fun defaultMobileUiLanguage(): String {
    return when (Locale.getDefault().language.lowercase(Locale.US)) {
        "vi" -> "vi"
        "ko" -> "ko"
        else -> "en"
    }
}

