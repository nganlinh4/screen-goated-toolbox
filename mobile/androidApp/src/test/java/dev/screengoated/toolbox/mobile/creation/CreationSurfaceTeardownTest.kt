package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationSurfaceTeardownTest {
    @Test
    fun ownerReleaseRunsEveryStepInOrderEvenAfterAFailure() {
        val calls = mutableListOf<String>()
        assertThrows(IllegalStateException::class.java) {
            releaseCreationOwner(
                closeOwner = { calls += "close"; throw IllegalStateException("journal busy") },
                releaseSources = { calls += "sources" },
                releaseSurface = { calls += "surface" },
            )
        }
        assertEquals(listOf("close", "sources", "surface"), calls)
    }

    @Test
    fun cancelAndCloseNeverTakeTheManagerLockOnTheCallingThread() {
        val viewModel = source("CreationNativeViewModel.kt")
        val cancel = viewModel.substringAfter("fun cancelSelected()")
            .substringBefore("\n    fun closeMiniApp()")
        assertTrue(cancel.contains("viewModelScope.launch(Dispatchers.IO)"))
        assertTrue(
            cancel.indexOf("launch(Dispatchers.IO)") < cancel.indexOf("manager.cancel("),
        )

        val close = viewModel.substringAfter("fun closeMiniApp()")
            .substringBefore("\n    fun segmentSelected()")
        val launched = close.substringAfter("creationSurfaceIoScope.launch")
        val beforeLaunch = close.substringBefore("creationSurfaceIoScope.launch")
        listOf("manager.closeOwner(", "releaseSurfaceSources(", "manager.releaseSurface(")
            .forEach { call ->
                assertFalse(call, beforeLaunch.contains(call))
                assertTrue(call, launched.contains(call))
            }
    }

    @Test
    fun draftSourceSyncDefersDiskCleanupOffTheCallingThread() {
        val viewModel = source("CreationNativeViewModel.kt")
        val sync = viewModel.substringAfter("private fun syncSourceHandles()")
            .substringBefore("\n    private fun ")
        assertTrue(sync.contains("updateSurfaceSourceLeases("))
        assertTrue(sync.contains("creationSurfaceIoScope.launch"))
        assertFalse(source("CreationFileStore.kt").contains("fun updateSurfaceSources("))
    }

    private fun source(name: String): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/$name",
        ).readText()
    }
}
