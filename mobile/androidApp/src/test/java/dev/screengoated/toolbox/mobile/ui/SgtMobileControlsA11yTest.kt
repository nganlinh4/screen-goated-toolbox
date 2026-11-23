package dev.screengoated.toolbox.mobile.ui

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SgtMobileControlsA11yTest {
    /** The shape-only theme toggle has no text, so its localized description must reach semantics. */
    @Test
    fun themeToggle_hasContentDescription() {
        val toggle = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/SgtMobileControls.kt",
        ).readText()
            .substringAfter("internal fun ThemeMorphToggle(")
            .substringBefore("\n}\n")
        assertTrue(toggle.contains("Modifier.semantics { this.contentDescription = contentDescription }"))
    }

    /** Icon-only actions announce what they do and which preset they act on. */
    @Test
    fun iconOnlyActionsAnnounceLocalizedLabels() {
        assertEquals("Delete: Translate", toolbarActionLabel("Delete", "Translate"))
        val ui = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/"
        val row = repoFile(ui + "ToolCategoryRow.kt").readText()
        listOf("favorite", "duplicate", "delete").forEach { action ->
            assertTrue(action, row.contains("toolbarActionLabel(it.$action, preset.name(lang))"))
        }
        val dialogClose = repoFile(ui + "ExpressiveDialogs.kt").readText()
            .substringAfter("internal fun ExpressiveDialogSurface(")
        assertTrue(dialogClose.contains("closeLabel: String,"))
        assertTrue(dialogClose.contains("contentDescription = label,"))
        val toggle = repoFile(ui + "MobileSettingsExpressive.kt").readText()
            .substringAfter("internal fun MorphingVisibilityToggleButton(")
            .substringBefore("\n}\n")
        assertTrue(toggle.contains("contentDescription = contentDescription,"))
        assertTrue(
            repoFile(ui + "MobileShellCards.kt").readText()
                .contains("if (visibleState[entry.index]) locale.apiKeyHide else locale.apiKeyShow"),
        )
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
