package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenCaptureFailurePolicyTest {
    @Test
    fun `retryable failures become visible only after the bounded grace period`() {
        val policy = ScreenCaptureFailurePolicy()

        assertFalse(policy.shouldPublish("temporary_failure", retryable = true))
        assertFalse(policy.shouldPublish("temporary_failure", retryable = true))
        assertTrue(policy.shouldPublish("temporary_failure", retryable = true))
    }

    @Test
    fun `a different retryable outcome starts a fresh grace period`() {
        val policy = ScreenCaptureFailurePolicy()

        repeat(2) { assertFalse(policy.shouldPublish("first_failure", retryable = true)) }
        assertFalse(policy.shouldPublish("second_failure", retryable = true))
    }

    @Test
    fun `non retryable failure is visible immediately`() {
        val policy = ScreenCaptureFailurePolicy()

        assertTrue(policy.shouldPublish("hard_failure", retryable = false))
    }

    @Test
    fun `successful frame resets accumulated failures`() {
        val policy = ScreenCaptureFailurePolicy()

        repeat(2) { assertFalse(policy.shouldPublish("temporary_failure", retryable = true)) }
        policy.reset()
        assertFalse(policy.shouldPublish("temporary_failure", retryable = true))
    }
}
