package dev.screengoated.toolbox.mobile.helpassistant

import dev.screengoated.toolbox.mobile.preset.GeneratedPresetModelCatalogData
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal val PRIMARY_MODEL: String = GeneratedPresetModelCatalogData.helpAssistantModelChain[0]
internal val FALLBACK_MODEL: String = GeneratedPresetModelCatalogData.helpAssistantModelChain[1]
const val MAX_OUTPUT_TOKENS = 4096

const val HELP_INDEX_URL =
    "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/help-index.json"

data class HelpAssistantRequest(
    val question: String,
    val uiLanguage: String,
    val geminiApiKey: String,
)

internal fun helpPlaceholder(locale: MobileLocaleText): String =
    locale.helpAssistantRestPlaceholder

internal fun helpLoadingMessage(locale: MobileLocaleText): String =
    locale.helpAssistantRestLoading

internal fun helpResultMarkdown(question: String, answer: String): String =
    "### $question\n\n$answer"

internal fun helpErrorMarkdown(message: String): String =
    "## ❌ Error\n\n$message"
