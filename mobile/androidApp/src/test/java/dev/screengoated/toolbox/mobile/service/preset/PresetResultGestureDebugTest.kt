package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Test

class PresetResultGestureDebugTest {
    /** No result page posts gestureDebug any more, so the host must not keep a handler for it. */
    @Test
    fun gestureDebugHasNoHandler() {
        val presetDir = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset")
        presetDir.walkTopDown().filter { it.isFile && it.extension == "kt" }.forEach { file ->
            val source = file.readText()
            assertFalse(file.name, source.contains("gestureDebug"))
            assertFalse(file.name, source.contains("logResultGestureDebug"))
        }
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
