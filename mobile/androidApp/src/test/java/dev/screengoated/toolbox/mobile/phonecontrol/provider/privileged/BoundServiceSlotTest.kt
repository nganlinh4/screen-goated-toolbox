package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class BoundServiceSlotTest {
    @Test
    fun bindTimeout_returnsRetryableFailureAndRebinds() = runTest {
        val slot = BoundServiceSlot<String>(isAlive = { true }, invalidBinderMessage = "invalid")
        var binds = 0
        var unbinds = 0
        val bind = {
            binds++
            if (binds == 2) slot.connected("service")
        }

        try {
            slot.await(timeoutMs = 100, bind = bind, onTimeout = { unbinds++ })
            fail("a bind that never connects must time out")
        } catch (_: ServiceBindTimeoutException) {
            // Not a CancellationException: the bridge maps it to a retryable failure.
        }
        assertEquals(1, unbinds)

        assertEquals("service", slot.await(timeoutMs = 100, bind = bind, onTimeout = { unbinds++ }))
        assertEquals(2, binds)
        assertEquals("service", slot.await(timeoutMs = 100, bind = bind, onTimeout = { unbinds++ }))
        assertEquals("a live service is reused", 2, binds)
        assertEquals(1, unbinds)
    }

    @Test
    fun invalidatedServiceBindsAgain() = runTest {
        val slot = BoundServiceSlot<String>(isAlive = { true }, invalidBinderMessage = "invalid")
        var binds = 0
        val bind = {
            binds++
            slot.connected("service-$binds")
        }
        assertEquals("service-1", slot.await(100, bind) {})
        slot.invalidate()
        assertEquals("service-2", slot.await(100, bind) {})
    }

    @Test
    fun invalidBinderFailsTheWaiter() = runTest {
        val slot = BoundServiceSlot<String>(isAlive = { true }, invalidBinderMessage = "invalid binder")
        try {
            slot.await(100, bind = { slot.connected(null) }, onTimeout = {})
            fail("an invalid binder must fail the bind")
        } catch (error: IllegalStateException) {
            assertTrue(error !is ServiceBindTimeoutException)
            assertEquals("invalid binder", error.message)
        }
    }
}
