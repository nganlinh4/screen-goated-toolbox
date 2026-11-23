package dev.screengoated.toolbox.mobile.ui.i18n

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class LocaleFieldUsageTest {
    @Test
    fun deletedFieldsStayDeleted() {
        val contracts = localeDir().resolve("MobileLocaleContracts.kt").readText()
        (FIRST_PASS_DELETED + SECOND_PASS_DELETED)
            .forEach { field -> assertFalse(field, Regex("""\bval $field:""").containsMatchIn(contracts)) }
    }

    /** Ratchet: no new contract field may be added without a reader outside the locale tables. */
    @Test
    fun everyContractFieldIsReferencedOutsideTheLocaleTables() {
        val fields = Regex("""^\s+val (\w+):""", RegexOption.MULTILINE)
            .findAll(localeDir().resolve("MobileLocaleContracts.kt").readText())
            .map { it.groupValues[1] }
            .toList()
        val words = SOURCE_ROOTS.asSequence()
            .map { repoFile(it) }
            .filter(File::isDirectory)
            .flatMap { root -> root.walkTopDown().filter { it.isFile && it.extension == "kt" } }
            .filterNot { it.parentFile == localeDir() && it.name in LOCALE_TABLES }
            .flatMap { WORD.findAll(it.readText()).map(MatchResult::value) }
            .toSet()

        val unread = fields.filterNot(words::contains).toSet()

        assertEquals("unread locale fields", emptySet<String>(), unread)
    }

    private fun localeDir(): File =
        repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n").canonicalFile

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }

    private companion object {
        val WORD = Regex("""\w+""")
        val SOURCE_ROOTS = listOf("main", "full", "play", "debug")
            .map { "mobile/androidApp/src/$it/java" }
        val LOCALE_TABLES = setOf(
            "MobileLocaleContracts.kt",
            "MobileLocaleSections.kt",
            "MobileLocaleEn.kt",
            "MobileLocaleVi.kt",
            "MobileLocaleKo.kt",
        )

        val FIRST_PASS_DELETED = listOf(
            "appSubtitle", "appTitle", "comingSoonLabel", "customModelsSave", "customModelsScanOllama", "dlReset",
        )

        /** Unread fields deleted in the second pass; they must not come back. */
        val SECOND_PASS_DELETED = listOf(
            "shellUtilitiesTitle", "shellDownloadedToolsDescription", "shellHelpDescription",
            "shellCredentialsDescription", "shellLiveDescription", "helpAssistantQuickOption",
            "helpAssistantDetailedOption", "helpAssistantScreenRecordOption", "helpAssistantAndroidOption",
            "helpAssistantRestOption", "helpAssistantScreenRecordPlaceholder", "helpAssistantAndroidPlaceholder",
            "helpAssistantScreenRecordLoading", "helpAssistantAndroidLoading", "translationGummyTranscriptTitle",
            "shellVoiceTitle", "shellVoiceDescription", "shellStatusIdle", "shellStatusActive",
            "shellStatusStarting", "shellStatusTranslating", "presetRuntimeSettingsAction",
            "usageStatsSettingsAction", "usageStatsEndpointCount", "presetRuntimeChainHint",
            "presetRuntimeSave", "presetRuntimeCancel", "customModelsAddOpenRouter",
            "customModelsImportOpenRouter", "ttsMethodLabel", "ttsLoadingVoices", "ttsInitializingVoices",
            "ttsResetToDefaultsLabel", "ttsPreviewTexts", "ttsInstructionsHint", "dlTitle", "dlFormatLabel",
            "dlSubsFoundHeader",
        )
    }
}
