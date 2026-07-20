package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlHandler
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlBrowserRegistryTest {
    @Test
    fun ordinaryAndroidBrowserBaselineHasRealHandlers() {
        val expected = mapOf(
            "browser_setup" to PhoneControlHandler.BROWSER_SETUP,
            "browser_status" to PhoneControlHandler.BROWSER_STATUS,
            "browser_read_page" to PhoneControlHandler.BROWSER_READ_PAGE,
            "browser_extract_page" to PhoneControlHandler.BROWSER_EXTRACT_PAGE,
            "browser_navigate" to PhoneControlHandler.BROWSER_NAVIGATE,
            "browser_history" to PhoneControlHandler.BROWSER_HISTORY,
        )

        expected.forEach { (name, handler) ->
            assertEquals(handler, PhoneControlToolRegistry.byName.getValue(name).handler)
        }
        assertFalse(PhoneControlToolRegistry.byName.getValue("browser_read_page").handler!!.mutating)
        assertTrue(PhoneControlToolRegistry.byName.getValue("browser_navigate").handler!!.mutating)
        assertTrue(PhoneControlToolRegistry.byName.getValue("browser_history").handler!!.mutating)
    }

    @Test
    fun cdpOnlyToolsStayExplicitlyUnavailable() {
        val cdpOnly = listOf(
            "browser_reset",
            "browser_wait_for",
            "browser_eval",
            "browser_open_tab",
            "browser_upload",
            "browser_tabs",
            "browser_switch_tab",
            "browser_close_tab",
            "browser_network",
            "browser_console",
        )

        cdpOnly.forEach { name ->
            val spec = PhoneControlToolRegistry.byName.getValue(name)
            assertNull("$name must not claim an ordinary Android implementation", spec.handler)
            assertEquals(listOf("browser_cdp"), spec.providerIds)
        }
    }

    @Test
    fun browserDependenciesAreDeclaredOnTheExactToolsThatUseThem() {
        val accessibilityDependencies = listOf("browser_setup", "browser_navigate")
        val customTabsDependencies = listOf(
            "browser_status",
            "browser_read_page",
            "browser_extract_page",
            "browser_history",
        )

        accessibilityDependencies.forEach { name ->
            assertEquals(
                setOf("accessibility"),
                PhoneControlToolRegistry.byName.getValue(name).dependencyProviderIds,
            )
        }
        customTabsDependencies.forEach { name ->
            assertEquals(
                setOf("custom_tabs_session"),
                PhoneControlToolRegistry.byName.getValue(name).dependencyProviderIds,
            )
        }
    }
}
