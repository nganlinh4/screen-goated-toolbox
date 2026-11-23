package dev.screengoated.toolbox.mobile.phonecontrol

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ProtectedCheckpointHandoffExitTest {
    @Test
    fun sealedDeadlineReportsTheFailureBeforeTheAbandonReleasesAGoal() {
        val order = mutableListOf<String>()
        phoneControlFailSealedCheckpoint(
            reportFailure = { order += "navigation_exhausted" },
            abandon = { order += "abandon_and_submit_pending_goal" },
        )
        assertEquals(listOf("navigation_exhausted", "abandon_and_submit_pending_goal"), order)

        val handoff = source("PhoneControlProtectedCheckpointHandoff.kt")
        val branch = handoff.substringAfter("reason=sealed_deadline\",").substringBefore("return")
        assertTrue(branch.contains("phoneControlFailSealedCheckpoint("))
        val failure = branch.indexOf("failNavigation(provider, \"sealed_deadline\")")
        assertTrue(failure in 0 until branch.indexOf("checkpoint.abandon("))
    }

    @Test
    fun sealedDeadlineIsBoundedAndInactiveBeforeSealing() {
        assertFalse(phoneControlProtectedSealedDeadlineReached(nowMs = 5_000L, sealedDeadlineMs = 0L))
        assertFalse(phoneControlProtectedSealedDeadlineReached(nowMs = 5_000L, sealedDeadlineMs = 5_001L))
        assertTrue(phoneControlProtectedSealedDeadlineReached(nowMs = 5_001L, sealedDeadlineMs = 5_001L))
    }

    @Test
    fun cancellingTheMonitorEndsTheCheckpointItOwns() {
        val handoff = source("PhoneControlProtectedCheckpointHandoff.kt")
        val cancel = handoff.substringAfter("private fun cancelMonitor(").substringBefore("\n    }")
        assertTrue(cancel.contains("if (endCheckpoint) abandonCheckpoint(reason)"))
        val abandon = handoff.substringAfter("private fun abandonCheckpoint(").substringBefore("\n    }")
        assertTrue(abandon.contains("checkpoint.abandon(sealed, runtime(), reason)"))
        // The sealed wait no longer re-arms a warning forever.
        assertFalse(handoff.contains("owned_action_pending"))
        assertTrue(handoff.contains("failNavigation(provider, \"sealed_deadline\")"))
    }

    @Test
    fun abandonReleasesTheRegistryTokenAndResumesTheRuntime() {
        val controller = source("PhoneControlProtectedCheckpointController.kt")
        val abandon = controller.substringAfter("fun abandon(").substringBefore("\n    }")
        assertTrue(abandon.contains("if (token != expectedToken) return false"))
        assertTrue(abandon.contains("PhoneControlProtectedCheckpointRegistry.end(expectedToken)"))
        assertTrue(abandon.contains("runtime?.resumeVisualEvidence()"))
        assertTrue(abandon.contains("onExit()"))
    }

    private fun source(name: String): String = File(repoRoot(), "$PHONE_CONTROL_DIR/$name").readText()

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current -> current.parentFile }
            .firstOrNull { File(it, PHONE_CONTROL_DIR).isDirectory }
            ?: error("Could not locate $PHONE_CONTROL_DIR from $workingDirectory")
    }

    private companion object {
        const val PHONE_CONTROL_DIR = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol"
    }
}
