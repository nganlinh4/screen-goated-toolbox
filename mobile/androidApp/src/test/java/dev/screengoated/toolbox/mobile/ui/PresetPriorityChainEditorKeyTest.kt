package dev.screengoated.toolbox.mobile.ui

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Source contract: Compose UI tests are not available to JVM unit tests in this module. */
class PresetPriorityChainEditorKeyTest {
    private val source = repoFile(
        "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/PresetPriorityChainEditor.kt",
    ).readText()

    @Test
    fun dragAcrossTwoSlots_keepsTheRowIdentity() {
        val rowKey = source.substringAfter("visibleChain.forEachIndexed { index, modelId ->")
            .substringBefore("DraggablePriorityModel(")
        val keyCall = rowKey.substringAfter("key(").substringBefore(") {")
        assertTrue(keyCall.contains("modelId"))
        assertTrue("a repeated id must still get a unique key", keyCall.contains("occurrence"))
        assertFalse("a reorder must not change the row key", keyCall.contains("index"))
    }

    @Test
    fun disposedRow_endsItsDrag() {
        val row = source.substringAfter("private fun DraggablePriorityModel(")
            .substringBefore("val dragModifier")
        val effect = row.substringAfter("DisposableEffect(")
        assertTrue(row.contains("DisposableEffect("))
        assertTrue(effect.contains("onDispose { if (draggingNow) currentEnd() }"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
