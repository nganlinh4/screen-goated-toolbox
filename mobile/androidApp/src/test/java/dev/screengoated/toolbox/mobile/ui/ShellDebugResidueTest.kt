package dev.screengoated.toolbox.mobile.ui

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Test

class ShellDebugResidueTest {
    private val shellFiles = listOf(
        "MobileShellSurface.kt",
        "MobileShellSections.kt",
        "AppsCarouselSection.kt",
        "AppsCarouselTiles.kt",
        "AppsCarouselDecoration.kt",
        "ToolsSection.kt",
        "ToolCategoryRow.kt",
    )

    /** Preset names are user content; the tools toolbar must not dump them to logcat. */
    @Test
    fun shellAndToolsSourcesCarryNoDebugLogging() {
        shellFiles.forEach { name ->
            val source = uiFile(name).readText()
            assertFalse(name, Regex("""Log\.[dv]\(""").containsMatchIn(source))
            assertFalse(name, source.contains("println("))
        }
    }

    /** The session toggle was always enabled; the constant flag threaded through four files is gone. */
    @Test
    fun alwaysTrueToggleFlagIsNotThreadedThroughTheShell() {
        shellFiles.forEach { name -> assertFalse(name, uiFile(name).readText().contains("canToggle")) }
    }

    private fun uiFile(name: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/$name")
    }
}
