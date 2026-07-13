package dev.screengoated.toolbox.mobile.ui.i18n

/**
 * Shared value types for the mobile locale text catalog.
 *
 * These are intentionally kept separate from [MobileLocaleText] and the per-language
 * factories so the catalog can grow without any single file becoming unwieldy.
 */

data class MobileUiLanguageOption(
    val code: String,
    val label: String,
)

data class MobileToolsHelpCopy(
    val title: String,
    val bubbleTitle: String,
    val bubbleDescription: String,
    val favoriteTitle: String,
    val favoriteDescription: String,
    val dismiss: String,
)

data class PreviewSelection(
    val index: Int,
    val text: String,
)
