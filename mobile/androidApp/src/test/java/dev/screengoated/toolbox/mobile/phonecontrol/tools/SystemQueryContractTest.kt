package dev.screengoated.toolbox.mobile.phonecontrol.tools

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SystemQueryContractTest {
    @Test
    fun `only canonical Windows domain query pairs are accepted`() {
        val supported = mapOf(
            "capabilities" to "list",
            "audio" to "active_sessions",
            "clipboard" to "text",
            "process" to "list_basic",
            "storage" to "volumes",
            "window" to "list",
        )

        supported.forEach { (domain, query) ->
            assertTrue(isSupportedSystemQuery(domain, query))
        }

        val allQueries = supported.values.toSet()
        supported.forEach { (domain, supportedQuery) ->
            allQueries.filterNot { it == supportedQuery }.forEach { otherQuery ->
                assertFalse(isSupportedSystemQuery(domain, otherQuery))
            }
        }
        assertFalse(isSupportedSystemQuery("unknown", "list"))
        assertFalse(isSupportedSystemQuery("window", "unknown"))
    }
}
