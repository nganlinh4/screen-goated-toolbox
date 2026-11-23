package dev.screengoated.toolbox.mobile.phonecontrol.tools

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityToolHandlersHygieneTest {
    @Test
    fun coordinateToolsNameTheirAlwaysFreshObservation() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/tools/AccessibilityToolHandlers.kt",
        ).readText()
        assertFalse(source.contains("currentOrFreshFrame"))
        assertFalse(source.contains("staleSurfaceExecution"))
        assertFalse(source.contains("import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome"))
        assertTrue(source.contains("freshObservationFrame(backend)"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
