package dev.screengoated.toolbox.mobile.creation.worker

import dev.screengoated.toolbox.mobile.creation.CreationWorkerEvent
import org.junit.Assert.assertEquals
import org.junit.Test

class CreationWorkerTerminalRelayTest {
    @Test
    fun `terminal event is retained until cleanup completes`() {
        val forwarded = mutableListOf<CreationWorkerEvent>()
        val relay = CreationWorkerTerminalRelay(forwarded::add)
        val progress = CreationWorkerEvent(event = "progress", stage = "generating")
        val recovery = CreationWorkerEvent(
            event = "failure",
            failureCode = "execution_lost",
        )

        relay.accept(progress)
        relay.accept(recovery)

        assertEquals(listOf(progress), forwarded)
        assertEquals(recovery, relay.complete(CreationWorkerEvent(event = "failure")))
    }

    @Test
    fun `first terminal event wins and later events are suppressed`() {
        val forwarded = mutableListOf<CreationWorkerEvent>()
        val relay = CreationWorkerTerminalRelay(forwarded::add)
        val success = CreationWorkerEvent(event = "success")

        relay.accept(success)
        relay.accept(CreationWorkerEvent(event = "failure"))
        relay.accept(CreationWorkerEvent(event = "progress"))

        assertEquals(emptyList<CreationWorkerEvent>(), forwarded)
        assertEquals(success, relay.complete(CreationWorkerEvent(event = "failure")))
    }
}
