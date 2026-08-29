package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationWorkerFollowUpTest {
    @Test
    fun `successful advertised follow-up keeps prepared worker available`() {
        assertTrue(
            creationWorkerCanServeFollowUp(
                CreationWorkerEvent(
                    event = "success",
                    availableActions = listOf("refine"),
                ),
            ),
        )
        assertTrue(
            creationWorkerCanServeFollowUp(
                CreationWorkerEvent(event = "success", canSegment = true),
            ),
        )
    }

    @Test
    fun `terminal result without a follow-up requires fresh preparation`() {
        assertFalse(creationWorkerCanServeFollowUp(CreationWorkerEvent(event = "success")))
        assertFalse(
            creationWorkerCanServeFollowUp(
                CreationWorkerEvent(event = "failure", canRefine = true),
            ),
        )
    }
}
