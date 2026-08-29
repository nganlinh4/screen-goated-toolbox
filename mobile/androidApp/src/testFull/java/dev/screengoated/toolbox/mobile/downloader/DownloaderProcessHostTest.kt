package dev.screengoated.toolbox.mobile.downloader

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DownloaderProcessHostTest {
    @Test
    fun sharedRecoveryFixtureRequiresIsolatedToolExecution() {
        val fixture = JSONObject(
            repoFile("parity-fixtures/video-downloader/recovery.json").readText(),
        )
        val command = YtDlpCommand("https://media.invalid/item")
        val arguments = buildYtDlpProcessArguments(command, "/runtime/ffmpeg", "/runtime/qjs")

        assertEquals("--ignore-config", arguments[0])
        assertEquals("--no-plugin-dirs", arguments[1])
        assertEquals(1, arguments.count { it == "--ignore-config" })
        assertEquals(1, arguments.count { it == "--no-plugin-dirs" })
        assertTrue(arguments.windowed(2).contains(listOf("--js-runtimes", "quickjs:/runtime/qjs")))
        assertTrue(fixture.getJSONObject("commandIsolation").getBoolean("ignoreUserConfiguration"))
        assertTrue(fixture.getJSONObject("commandIsolation").getBoolean("disableUserPluginDirectories"))
        assertTrue(fixture.getJSONObject("commandIsolation").getBoolean("managedJavaScriptRuntime"))
        assertTrue(fixture.getJSONObject("commandIsolation").getBoolean("managedChallengeScripts"))
        assertEquals(1, fixture.getJSONObject("failure").getInt("maximumRetries"))
    }

    @Test
    fun processErrorKeepsTheUsefulBoundedTail() {
        val stderr = (0 until 20).joinToString("\n") { "diagnostic $it" }
        val error = boundedYtDlpError(stderr, 2)

        assertFalse(error.contains("diagnostic 7"))
        assertTrue(error.contains("diagnostic 8"))
        assertTrue(error.contains("diagnostic 19"))
        assertEquals("yt-dlp exited with code 2", boundedYtDlpError(" \n", 2))
    }
}

private fun repoFile(relativePath: String): File {
    var current = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
    while (true) {
        val candidate = File(current, relativePath)
        if (candidate.exists()) return candidate
        current = current.parentFile ?: error("Could not find $relativePath")
    }
}
