package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CreationWorkerPreparationOrderTest {
    @Test
    fun `first empty slot starts first`() {
        assertEquals(0, nextCreationPreparationSlot(listOf(emptySlot(), emptySlot())))
    }

    @Test
    fun `duplicate signal cannot start another slot while one is binding`() {
        assertNull(
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(binding = true), emptySlot()),
            ),
        )
    }

    @Test
    fun `ready slot allows the next slot to warm`() {
        assertEquals(
            1,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(connected = true, ready = true), emptySlot()),
            ),
        )
    }

    @Test
    fun `busy slot allows parallel capacity to warm`() {
        assertEquals(
            1,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(connected = true, busy = true), emptySlot()),
            ),
        )
    }

    @Test
    fun `failed preparation releases its tool lane before retry`() {
        assertNull(
            activeCreationPreparationAfterFailure(
                CreationTool.IMAGE_CREATOR,
                CreationTool.IMAGE_CREATOR,
            ),
        )
        assertEquals(
            CreationTool.IMAGE_TO_3D,
            activeCreationPreparationAfterFailure(
                CreationTool.IMAGE_TO_3D,
                CreationTool.IMAGE_CREATOR,
            ),
        )
    }

    @Test
    fun `exhausted preparation remains failed until explicit restart`() {
        val failures = CreationPreparationFailureRegistry()

        assertEquals(true, failures.markFailed(CreationTool.IMAGE_CREATOR))
        assertEquals(true, failures.isFailed(CreationTool.IMAGE_CREATOR))
        assertEquals(
            setOf(CreationTool.IMAGE_TO_3D),
            failures.available(
                setOf(CreationTool.IMAGE_TO_3D, CreationTool.IMAGE_CREATOR),
            ),
        )
        assertEquals(true, failures.restart(CreationTool.IMAGE_CREATOR))
        assertEquals(false, failures.isFailed(CreationTool.IMAGE_CREATOR))
    }

    private fun emptySlot() = CreationPreparationSlotState(
        connected = false,
        binding = false,
        ready = false,
        busy = false,
    )
}
