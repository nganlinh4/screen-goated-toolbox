package dev.screengoated.toolbox.mobile.phonecontrol

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlExternalGoalSubmissionTest {
    private val slot = PhoneControlExternalGoalSlot(now = { 0L })

    @Test
    fun checkpointActive_keepsGoalQueued() {
        assertTrue(slot.offer("open the timer"))
        var offered = 0
        assertNull(slot.submitWhen(canAct = false) { offered += 1; true })
        assertEquals(0, offered)
        assertEquals("open the timer", slot.peek())
    }

    @Test
    fun rejectedSubmission_keepsGoalQueued() {
        assertTrue(slot.offer("open the timer"))
        assertEquals(PhoneControlExternalGoalSubmission(false, 14), slot.submitWhen(canAct = true) { false })
        assertEquals("open the timer", slot.peek())
    }

    @Test
    fun acceptedSubmission_consumesGoal() {
        assertTrue(slot.offer("open the timer"))
        val submitted = mutableListOf<String>()
        val result = slot.submitWhen(canAct = true) { submitted += it; true }
        assertEquals(PhoneControlExternalGoalSubmission(true, 14), result)
        assertEquals(listOf("open the timer"), submitted)
        assertNull(slot.peek())
        assertNull(slot.submitWhen(canAct = true) { true })
    }

    @Test
    fun serviceGatesOnCheckpointAndRetriesOnExit() {
        val service = source("PhoneControlService.kt")
        assertTrue(service.contains("PhoneControlProtectedCheckpointController(onExit = ::submitPendingExternalGoal)"))
        assertTrue(service.contains("PhoneControlExternalGoalRegistry.submitWhen(!protectedCheckpoint.active)"))
        val restore = source("PhoneControlProtectedCheckpointController.kt")
            .substringAfter("private fun restore(")
            .substringBefore("\n    }\n")
        assertTrue(restore.indexOf("onExit()") > restore.indexOf("runtime.resumeVisualEvidence()"))
        val abort = source("ui/PhoneControlActivationCoordinator.kt")
            .substringAfter("fun PhoneControlActivity.abortActivation(")
            .substringBefore("\n}\n")
        assertTrue(abort.contains("PhoneControlExternalGoalRegistry.clear()"))
    }

    @Test
    fun runtimeErrorTeardownDropsTheHeldGoal() {
        val runtimeError = source("PhoneControlService.kt")
            .substringAfter("if (!snapshot.running && snapshot.phase == PhoneControlRuntimePhase.ERROR) {")
            .substringBefore("\n        }\n")
        assertTrue(runtimeError.contains("PhoneControlExternalGoalRegistry.clear()"))
        assertTrue(
            runtimeError.indexOf("PhoneControlExternalGoalRegistry.clear()") <
                runtimeError.indexOf("requestServiceTeardown()"),
        )
    }

    private fun source(path: String): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/$path",
        ).readText()
    }
}
