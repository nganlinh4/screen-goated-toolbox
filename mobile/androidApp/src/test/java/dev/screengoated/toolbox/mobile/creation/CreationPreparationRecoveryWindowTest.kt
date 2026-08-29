package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationPreparationRecoveryWindowTest {
    @Test
    fun `temporary capacity pause preserves demand until the bounded deadline`() {
        var now = 1_000L
        val recovery = CreationPreparationRecoveryWindow(500L) { now }

        assertTrue(recovery.shouldRetry("job", 10_000L))
        now = 1_499L
        assertTrue(recovery.shouldRetry("job", 10_000L))
        now = 1_500L
        assertFalse(recovery.shouldRetry("job", 10_000L))
    }

    @Test
    fun `whole job deadline shortens capacity recovery`() {
        var now = 2_000L
        val recovery = CreationPreparationRecoveryWindow(5_000L) { now }

        assertTrue(recovery.shouldRetry("job", 2_100L))
        now = 2_100L
        assertFalse(recovery.shouldRetry("job", 2_100L))
    }

    @Test
    fun `assignment cancellation and owner close discard recovery state`() {
        var now = 3_000L
        val recovery = CreationPreparationRecoveryWindow(100L) { now }

        assertTrue(recovery.shouldRetry("assigned", 10_000L))
        assertTrue(recovery.shouldRetry("cancelled", 10_000L))
        recovery.clear("assigned")
        recovery.retain(emptySet())
        now = 3_090L

        assertTrue(recovery.shouldRetry("assigned", 10_000L))
        assertTrue(recovery.shouldRetry("cancelled", 10_000L))
    }
}
