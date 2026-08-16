package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Test

class CreationWorkerPreparationOrderTest {
    @Test
    fun `first empty slot starts first`() {
        assertEquals(
            0,
            nextCreationPreparationSlot(listOf(emptySlot(), emptySlot()), 1, 1),
        )
    }

    @Test
    fun `exclusive preparation waits while another slot is binding`() {
        assertEquals(
            null,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(binding = true), emptySlot()),
                2,
                1,
            ),
        )
    }

    @Test
    fun `parallel preparation starts an independent binding slot`() {
        assertEquals(
            1,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(binding = true), emptySlot()),
                2,
                2,
            ),
        )
    }

    @Test
    fun `one requested worker does not eagerly warm a second slot`() {
        assertEquals(
            null,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(connected = true, ready = true), emptySlot()),
                1,
                1,
            ),
        )
    }

    @Test
    fun `one busy worker satisfies one requested slot`() {
        assertEquals(
            null,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(connected = true, busy = true), emptySlot()),
                1,
                1,
            ),
        )
    }

    @Test
    fun `busy slot allows parallel capacity to warm`() {
        assertEquals(
            1,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(connected = true, busy = true), emptySlot()),
                2,
                1,
            ),
        )
    }

    @Test
    fun `connected preparation consumes an exclusive preparation lane`() {
        assertEquals(
            null,
            nextCreationPreparationSlot(
                listOf(emptySlot().copy(connected = true), emptySlot()),
                2,
                1,
            ),
        )
    }

    @Test
    fun `required recovery slot is prepared before a generic empty slot`() {
        assertEquals(
            1,
            nextCreationPreparationSlot(
                listOf(emptySlot(), emptySlot()),
                requestedCapacity = 1,
                maximumConcurrentPreparations = 1,
                requiredSlots = setOf(1),
            ),
        )
        assertEquals(
            false,
            creationPreparationCapacitySatisfied(
                listOf(emptySlot().copy(connected = true, ready = true), emptySlot()),
                requestedCapacity = 1,
                requiredSlots = setOf(1),
            ),
        )
    }

    @Test
    fun `capacity shrink retires idle initialization before ready capacity`() {
        assertEquals(
            listOf(1),
            creationPreparationRetirementSlots(
                listOf(
                    emptySlot().copy(connected = true, ready = true),
                    emptySlot().copy(binding = true),
                ),
                requestedCapacity = 1,
            ),
        )
        assertEquals(
            listOf(1),
            creationPreparationRetirementSlots(
                listOf(
                    emptySlot().copy(connected = true, busy = true),
                    emptySlot().copy(connected = true, ready = true),
                ),
                requestedCapacity = 1,
            ),
        )
        assertEquals(
            listOf(0),
            creationPreparationRetirementSlots(
                listOf(
                    emptySlot().copy(connected = true, ready = true),
                    emptySlot().copy(connected = true, busy = true),
                ),
                requestedCapacity = 1,
                requiredSlots = setOf(1),
            ),
        )
    }

    @Test
    fun `worker keys map exact recovery lanes`() {
        assertEquals(
            setOf(1),
            creationRequiredSlotIndexes(
                listOf("image-0", "image-1"),
                setOf("image-1"),
            ),
        )
        assertEquals(
            emptySet<Int>(),
            creationRequiredSlotIndexes(
                listOf("image-0", "image-1"),
                setOf("svg-1"),
            ),
        )
    }

    @Test
    fun `failure preserves an independent ready or preparing lane`() {
        assertEquals(
            true,
            hasIndependentPreparationLane(
                listOf(emptySlot().copy(connected = true), emptySlot().copy(ready = true)),
                failedSlot = 0,
            ),
        )
        assertEquals(
            true,
            hasIndependentPreparationLane(
                listOf(emptySlot().copy(connected = true), emptySlot().copy(binding = true)),
                failedSlot = 0,
            ),
        )
        assertEquals(
            false,
            hasIndependentPreparationLane(
                listOf(emptySlot().copy(connected = true), emptySlot()),
                failedSlot = 0,
            ),
        )
    }

    @Test
    fun `failed preparation releases its tool lane before retry`() {
        assertEquals(
            null,
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

    @Test
    fun `runtime delivery failure closes every retained preparation lane`() {
        val failures = CreationPreparationFailureRegistry()
        failures.markFailed(
            setOf(CreationTool.IMAGE_TO_3D, CreationTool.IMAGE_TO_SVG),
        )

        assertEquals(true, failures.isFailed(CreationTool.IMAGE_TO_3D))
        assertEquals(true, failures.isFailed(CreationTool.IMAGE_TO_SVG))
        assertEquals(false, failures.isFailed(CreationTool.IMAGE_CREATOR))
    }

    private fun emptySlot() = CreationPreparationSlotState(
        connected = false,
        binding = false,
        ready = false,
        busy = false,
    )
}
