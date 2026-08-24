package dev.screengoated.toolbox.mobile.model

import kotlinx.serialization.Serializable
import java.util.Locale

const val DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT: Int = 90
const val MIN_RESULT_OVERLAY_OPACITY_PERCENT: Int = 10
const val MAX_RESULT_OVERLAY_OPACITY_PERCENT: Int = 100

fun Int.normalizedResultOverlayOpacityPercent(): Int =
    coerceIn(MIN_RESULT_OVERLAY_OPACITY_PERCENT, MAX_RESULT_OVERLAY_OPACITY_PERCENT)

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
    val overlayOpacityPercent: Int = DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT,
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
