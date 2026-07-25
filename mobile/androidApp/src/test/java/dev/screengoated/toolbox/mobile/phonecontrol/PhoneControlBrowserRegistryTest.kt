package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlHandler
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
    fun cdpToolsAndPublicResearchHaveRealHandlers() {
        val expected = mapOf(
            "browser_reset" to PhoneControlHandler.BROWSER_RESET,
            "browser_wait_for" to PhoneControlHandler.BROWSER_WAIT_FOR,
            "browser_eval" to PhoneControlHandler.BROWSER_EVAL,
            "browser_open_tab" to PhoneControlHandler.BROWSER_OPEN_TAB,
            "browser_upload" to PhoneControlHandler.BROWSER_UPLOAD,
            "browser_tabs" to PhoneControlHandler.BROWSER_TABS,
            "browser_switch_tab" to PhoneControlHandler.BROWSER_SWITCH_TAB,
            "browser_close_tab" to PhoneControlHandler.BROWSER_CLOSE_TAB,
            "browser_network" to PhoneControlHandler.BROWSER_NETWORK,
            "browser_console" to PhoneControlHandler.BROWSER_CONSOLE,
        )

        expected.forEach { (name, handler) ->
            val spec = PhoneControlToolRegistry.byName.getValue(name)
            assertEquals(handler, spec.handler)
            assertEquals(listOf("browser_cdp"), spec.providerIds)
        }
        val research = PhoneControlToolRegistry.byName.getValue("research_web")
        assertEquals(PhoneControlHandler.RESEARCH_WEB, research.handler)
        assertEquals(listOf("direct_web_research"), research.providerIds)
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
