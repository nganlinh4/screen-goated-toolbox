package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.io.File

class ToolbarAndDonateLocaleTest {
    @Test
    fun donateActionsUseTheWindowsLocaleText() {
        val windowsCopyAndQr = mapOf(
            "en" to ("Copy account number" to "Open VietQR code"),
            "ko" to ("계좌번호 복사" to "VietQR 코드 열기"),
            "vi" to ("Sao chép STK" to "Mở mã VietQR"),
        )
        windowsCopyAndQr.forEach { (language, expected) ->
            val locale = MobileLocaleText.forLanguage(language)
            assertEquals(language, expected, locale.donateActions.copyAccount to locale.donateActions.openQr)
        }
    }

    @Test
    fun toolbarLabelsAreLocalizedForEveryLanguage() {
        val english = MobileLocaleText.forLanguage("en")
        assertEquals(
            listOf("Create", "Help", "Duplicate", "Favorite", "Delete"),
            english.toolbarLabels(),
        )
        listOf("ko", "vi").forEach { language ->
            val labels = MobileLocaleText.forLanguage(language).toolbarLabels()
            labels.zip(english.toolbarLabels()).forEach { (translated, base) ->
                assertFalse("$language keeps English '$base'", translated == base)
            }
        }
    }

    @Test
    fun toolsAndDonateSectionsCarryNoInlineTranslations() {
        listOf("ToolsSection.kt", "DonateSection.kt").forEach { name ->
            val source = File(repoRoot(), "$UI_DIR/$name").readText()
            // Preset names carry their own per-language data; only literal UI text is banned.
            assertFalse(name, Regex("when \\(lang\\) \\{ \"vi\" -> \"").containsMatchIn(source))
            assertFalse(name, source.contains("\"Sao chép STK\""))
            assertFalse(name, source.contains("\"Mở mã VietQR\""))
        }
    }

    private fun MobileLocaleText.toolbarLabels() = with(toolsToolbar) {
        listOf(create, help, duplicate, favorite, delete)
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current -> current.parentFile }
            .firstOrNull { File(it, "$UI_DIR/ToolsSection.kt").exists() }
            ?: error("Could not locate $UI_DIR from $workingDirectory")
    }

    private companion object {
        const val UI_DIR = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui"
    }
}
