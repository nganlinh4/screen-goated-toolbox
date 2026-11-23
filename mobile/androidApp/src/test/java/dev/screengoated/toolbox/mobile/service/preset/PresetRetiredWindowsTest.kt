package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetRetiredWindowsTest {
    @Test
    fun indicatorDoesNotReturnAfterTheSessionShowedAResult() {
        val gate = PresetProcessingIndicatorGate()

        assertTrue(gate.shouldShow("s1", executing = true, hasResultWindow = false))
        assertFalse(gate.shouldShow("s1", executing = true, hasResultWindow = true))
        // Every window was retired by skipped downstream blocks; the graph still runs.
        assertFalse(gate.shouldShow("s1", executing = true, hasResultWindow = false))
        assertFalse(gate.shouldShow("s1", executing = false, hasResultWindow = false))

        // A new session gets its own indicator.
        assertTrue(gate.shouldShow("s2", executing = true, hasResultWindow = false))
    }

    @Test
    fun graphRetiredWindowsKeepThePresetActive() {
        val base = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset"
        val module = File(repoRoot(), "$base/PresetOverlayResultModule.kt").readText()
        val emptyBranch = module.substringAfter("if (windowsToRender.isEmpty()) {").substringBefore("return")
        assertTrue(emptyBranch.contains("closeResultWindowSupport(it, notifyWhenEmpty = false)"))

        val lifecycle = File(repoRoot(), "$base/PresetOverlayResultLifecycleSupport.kt").readText()
        assertTrue(lifecycle.contains("if (notifyWhenEmpty && resultWindows.isEmpty()) {"))

        val controller = File(repoRoot(), "$base/PresetOverlayController.kt").readText()
        assertTrue(controller.contains("processingIndicatorGate.shouldShow(state.sessionId, state.isExecuting, hasAnyResultWindow)"))
    }

    private fun repoRoot(): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) directory = requireNotNull(directory.parentFile)
        return directory
    }
}
