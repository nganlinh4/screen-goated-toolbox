package dev.screengoated.toolbox.mobile.service

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SgtAccessibilityServiceSurfaceTest {
    /** Auto-paste goes through ProvisionalPasteSession; the old direct-append path stays deleted. */
    @Test
    fun legacyAppendPathIsGone() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/SgtAccessibilityService.kt",
        ).readText()
        listOf(
            "appendTextToFocusedField",
            "warnNoWritableTarget",
            "NO_WRITABLE_TARGET_COOLDOWN_MS",
            "lastNoWritableTargetToastAtMs",
        ).forEach { symbol -> assertFalse(symbol, source.contains(symbol)) }
    }

    /** Selection capture and paste never act on SGT's own overlay windows. */
    @Test
    fun everyWindowScanSkipsOwnWindows() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/SgtAccessibilityService.kt",
        ).readText()
        val loops = source.split("for (window in windows) {").drop(1)
        assertEquals(4, loops.size)
        loops.forEachIndexed { index, loop ->
            val head = loop.substringBefore("\n        }\n")
            val skip = head.indexOf("if (pkg == packageName) { continue }")
            assertTrue("window loop $index must skip own windows", skip >= 0)
            assertTrue("window loop $index skips before acting", skip < head.indexOf("performAction").let { if (it < 0) Int.MAX_VALUE else it })
            assertTrue("window loop $index skips before searching", skip < head.indexOf("COPY_ACTION_LABELS").let { if (it < 0) Int.MAX_VALUE else it })
        }
    }

    /**
     * The paste fallback writes clipboard text into another app's field, so it
     * must never resolve a clipboard content URI with SGT's own identity.
     */
    @Test
    fun pasteFallbackNeverResolvesClipboardUris() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/SgtAccessibilityService.kt",
        ).readText()
        assertFalse(source.contains("coerceToText("))
        val reader = source.substringAfter("private fun getClipboardText(): String? {")
            .substringBefore("\n    }\n")
        assertTrue(reader.contains("getItemAt(0)?.text?.toString()"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
