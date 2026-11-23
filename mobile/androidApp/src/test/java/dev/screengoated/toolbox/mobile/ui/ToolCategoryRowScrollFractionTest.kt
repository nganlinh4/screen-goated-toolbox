package dev.screengoated.toolbox.mobile.ui

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ToolCategoryRowScrollFractionTest {
    @Test
    fun addedPreset_updatesEdgeFadeFraction() {
        assertEquals(1f, carouselScrollFraction(currentItem = 2, presetCount = 3), 0f)
        // One preset added: the same position is no longer the end of the row.
        assertEquals(2f / 3f, carouselScrollFraction(currentItem = 2, presetCount = 4), 1e-6f)
        assertEquals(0f, carouselScrollFraction(currentItem = 0, presetCount = 1), 0f)

        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/ToolCategoryRow.kt",
        ).readText()
        assertTrue(source.contains("val scrollFraction by remember(presetCount) {"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
