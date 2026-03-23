package dev.screengoated.toolbox.mobile.helpassistant

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

enum class HelpAssistantBucket(
    val wireId: String,
    val rawFileName: String,
    val responseIcon: String,
    val presetPrompt: String,
) {
    SCREEN_RECORD(
        wireId = "screen-record",
        rawFileName = "repomix-screen-recorder.xml",
        responseIcon = "\uD83C\uDFAC",
        presetPrompt = "Ask SGT Record",
    ),
    ANDROID(
        wireId = "android",
        rawFileName = "repomix-android.xml",
        responseIcon = "\uD83D\uDCF1",
        presetPrompt = "Ask SGT Android",
    ),
    REST(
        wireId = "rest",
        rawFileName = "repomix-rest.xml",
        responseIcon = "❓",
        presetPrompt = "Ask SGT",
    ),
}

data class HelpAssistantRequest(
    val bucket: HelpAssistantBucket,
    val question: String,
    val uiLanguage: String,
    val geminiApiKey: String,
)

internal fun HelpAssistantBucket.label(locale: MobileLocaleText): String = when (this) {
    HelpAssistantBucket.SCREEN_RECORD -> locale.helpAssistantScreenRecordOption
    HelpAssistantBucket.ANDROID -> locale.helpAssistantAndroidOption
    HelpAssistantBucket.REST -> locale.helpAssistantRestOption
}

internal fun HelpAssistantBucket.placeholder(locale: MobileLocaleText): String = when (this) {
    HelpAssistantBucket.SCREEN_RECORD -> locale.helpAssistantScreenRecordPlaceholder
    HelpAssistantBucket.ANDROID -> locale.helpAssistantAndroidPlaceholder
    HelpAssistantBucket.REST -> locale.helpAssistantRestPlaceholder
}

internal fun HelpAssistantBucket.loadingMessage(locale: MobileLocaleText): String = when (this) {
    HelpAssistantBucket.SCREEN_RECORD -> locale.helpAssistantScreenRecordLoading
    HelpAssistantBucket.ANDROID -> locale.helpAssistantAndroidLoading
    HelpAssistantBucket.REST -> locale.helpAssistantRestLoading
}

internal fun HelpAssistantBucket.rawUrl(): String =
    "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/$rawFileName"

internal fun HelpAssistantBucket.resultMarkdown(
    locale: MobileLocaleText,
    question: String,
    answer: String,
): String {
    return "## $responseIcon ${label(locale)}\n\n### $question\n\n$answer"
}

internal fun HelpAssistantBucket.errorMarkdown(message: String): String =
    "## ❌ Error\n\n$message"

internal fun helpAssistantBucketFromWireId(value: String?): HelpAssistantBucket? =
    HelpAssistantBucket.entries.firstOrNull { it.wireId == value }
