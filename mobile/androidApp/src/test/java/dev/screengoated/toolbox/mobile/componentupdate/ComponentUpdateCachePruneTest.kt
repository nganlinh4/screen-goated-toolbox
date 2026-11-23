package dev.screengoated.toolbox.mobile.componentupdate

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ComponentUpdateCachePruneTest {
    @Test
    fun store65_prunesAndStillLoads() {
        val names = (1..65).map(::catalogName)
        val newest = catalogName(65)

        val pruned = cachedCatalogsToPrune(names, keep = newest)

        assertEquals(61, pruned.size)
        assertEquals(listOf(62, 63, 64, 65).map(::catalogName), names - pruned.toSet())
        assertFalse(newest in pruned)
    }

    @Test
    fun justStoredCatalogIsNeverPruned() {
        val names = (1..10).map(::catalogName)
        val stored = catalogName(2)

        val kept = names - cachedCatalogsToPrune(names, keep = stored).toSet()

        assertEquals(listOf(2, 7, 8, 9, 10).map(::catalogName), kept)
    }

    private fun catalogName(sequence: Int) =
        "sgt-component-catalog-v%06d-0123456789abcdef.json".format(sequence)
}
