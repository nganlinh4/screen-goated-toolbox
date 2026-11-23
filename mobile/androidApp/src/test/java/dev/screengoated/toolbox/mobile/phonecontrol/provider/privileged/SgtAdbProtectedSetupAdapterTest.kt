package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.fail
import org.junit.Test

class SgtAdbProtectedSetupAdapterTest {
    @Test
    fun cancelDuringReconnect_propagates() = runBlocking {
        try {
            reconcileSgtAdbPairing { throw CancellationException("checkpoint cancelled") }
            fail("checkpoint cancellation must not be swallowed")
        } catch (cancelled: CancellationException) {
            assertEquals("checkpoint cancelled", cancelled.message)
        }
    }

    @Test
    fun failedReconnect_fallsThroughToPairing() = runBlocking {
        assertNull(reconcileSgtAdbPairing { throw IOException("adb offline") })
        val ready = SgtAdbBridgeProbe(CapabilityState.READY, SgtAdbBridgeCondition.READY)
        assertEquals(ready, reconcileSgtAdbPairing { ready })
    }
}
