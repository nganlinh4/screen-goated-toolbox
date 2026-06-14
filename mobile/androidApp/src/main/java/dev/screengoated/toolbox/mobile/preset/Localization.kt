package dev.screengoated.toolbox.mobile.preset

/**
 * Central tri-language string selector.
 *
 * Picks the Vietnamese / Korean / English variant based on the UI language code.
 * Use this instead of re-declaring a local `when (lang) { "vi" -> ..; "ko" -> ..; else -> en }`
 * switch in every preset surface.
 */
fun triLang(lang: String, en: String, vi: String, ko: String): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}
