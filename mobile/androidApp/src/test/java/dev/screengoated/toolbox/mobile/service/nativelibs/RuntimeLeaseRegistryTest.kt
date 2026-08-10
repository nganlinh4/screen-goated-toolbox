package dev.screengoated.toolbox.mobile.service.nativelibs

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RuntimeLeaseRegistryTest {
    @Test
    fun `unused runtime becomes removable immediately`() {
        val ready = mutableListOf<String>()
        val registry = RuntimeLeaseRegistry<String>(ready::add)

        registry.requestRemoval("runtime")

        assertEquals(listOf("runtime"), ready)
        assertTrue(registry.isRemovalPending("runtime"))
        registry.completeRemoval("runtime")
        assertFalse(registry.isRemovalPending("runtime"))
    }

    @Test
    fun `removal waits for every active lease`() {
        val ready = mutableListOf<String>()
        val registry = RuntimeLeaseRegistry<String>(ready::add)
        val first = requireNotNull(registry.acquire(listOf("runtime")))
        val second = requireNotNull(registry.acquire(listOf("runtime")))

        registry.requestRemoval("runtime")
        assertTrue(registry.isRemovalPending("runtime"))
        assertTrue(registry.isInUse("runtime"))
        assertTrue(ready.isEmpty())

        first.close()
        assertTrue(ready.isEmpty())
        second.close()
        assertEquals(listOf("runtime"), ready)
        assertFalse(registry.isInUse("runtime"))
    }

    @Test
    fun `pending removal rejects new use and lease close is idempotent`() {
        val ready = mutableListOf<String>()
        val registry = RuntimeLeaseRegistry<String>(ready::add)
        val lease = requireNotNull(registry.acquire(listOf("runtime")))

        registry.requestRemoval("runtime")
        assertNull(registry.acquire(listOf("runtime")))

        lease.close()
        lease.close()
        assertEquals(listOf("runtime"), ready)
    }

    @Test
    fun `failed physical removal can restore availability`() {
        val registry = RuntimeLeaseRegistry<String> {}

        registry.requestRemoval("runtime")
        registry.cancelRemoval("runtime")

        assertFalse(registry.isRemovalPending("runtime"))
        assertNotNull(registry.acquire(listOf("runtime")))
    }

    @Test
    fun `deferred uninstall remains pending until payload is absent`() {
        assertEquals(
            DeferredRemovalState.REMOVAL_PENDING,
            deferredRemovalState(installed = true, removalRequested = true),
        )
        assertEquals(
            DeferredRemovalState.INSTALLED,
            deferredRemovalState(installed = true, removalRequested = false),
        )
        assertEquals(
            DeferredRemovalState.MISSING,
            deferredRemovalState(installed = false, removalRequested = true),
        )
    }
}
