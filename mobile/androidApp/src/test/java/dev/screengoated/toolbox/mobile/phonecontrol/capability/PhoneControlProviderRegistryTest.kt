package dev.screengoated.toolbox.mobile.phonecontrol.capability

import org.junit.Assert.assertEquals
import org.junit.Test

class PhoneControlProviderRegistryTest {
    @Test
    fun `active projection is reported ready in the runtime capability snapshot`() {
        assertEquals(
            CapabilityState.READY,
            mediaProjectionCapabilityState(isReady = true),
        )
        assertEquals(
            CapabilityState.NEEDS_USER_STEP,
            mediaProjectionCapabilityState(isReady = false),
        )
    }
}
