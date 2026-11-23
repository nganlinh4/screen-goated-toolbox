package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetOverlayCopyToastTest {
    @Test
    fun autoCopyToast_isGeneric() {
        assertEquals("Copied", copyToastText("en", copied = true))
        assertEquals("Đã sao chép", copyToastText("vi", copied = true))
        assertEquals("복사됨", copyToastText("ko", copied = true))
        assertEquals("Could not copy result.", copyToastText("en", copied = false))
        listOf("en", "vi", "ko").forEach { language ->
            assertFalse(copyToastText(language, copied = true).contains("ranscript"))
        }
    }

    @Test
    fun autoCopyUsesTheSharedToast() {
        val controller = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetOverlayController.kt",
        ).readText()
        assertTrue(controller.contains("copyToastText(uiLanguage(), copied)"))
        assertFalse(controller.contains("Transcript copied."))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
