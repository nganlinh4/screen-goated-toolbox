package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import org.junit.Assert.assertEquals
import org.junit.Test

class ShizukuCommandBridgeTest {
    @Test
    fun missingPackageAndStoppedServiceHaveDifferentRecoverySteps() {
        val missing = shizukuBinderUnavailable(packageInstalled = false)
        val stopped = shizukuBinderUnavailable(packageInstalled = true)

        assertEquals(CapabilityState.UNAVAILABLE, missing.state)
        assertEquals("Install Shizuku to add shell authority.", missing.requiredUserStep)
        assertEquals(CapabilityState.NEEDS_USER_STEP, stopped.state)
        assertEquals("Start Shizuku or Sui.", stopped.requiredUserStep)
    }
}
