package dev.screengoated.toolbox.mobile

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class MainViewModelSurfaceTest {
    @Test
    fun unreferencedHandlersStayDeleted() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/MainViewModel.kt",
        ).readText()
        listOf(
            "fun onSourceModeSelected(",
            "fun onDisplayModeSelected(",
            "fun onTargetLanguageSelected(",
            "fun onThemeSelected(",
            "fun onRealtimeTts",
        ).forEach { declaration -> assertFalse(declaration, source.contains(declaration)) }
    }

    @Test
    fun initSetsSafeDisplayModeOnce() {
        val init = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/MainViewModel.kt",
        ).readText().substringAfter("    init {").substringBefore("\n    }\n")
        assertEquals(1, Regex("DisplayMode\\.IN_APP_MIRROR").findAll(init).count())
        assertFalse(init.contains("ensureSafePlayDefaults"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
