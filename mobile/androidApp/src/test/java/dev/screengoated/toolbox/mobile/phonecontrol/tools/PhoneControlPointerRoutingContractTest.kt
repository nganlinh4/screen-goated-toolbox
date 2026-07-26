package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlPointerRoutingContractTest {
    @Test
    fun coordinateToolsUseAccessibilityBeforeSelectedElevatedEffectBackends() {
        val expected = listOf(
            "accessibility",
            "sgt_adb_bridge",
            "shizuku_shell",
            "root_bridge",
        )

        listOf("click_at", "scroll", "drag").forEach { tool ->
            assertEquals(expected, PhoneControlToolRegistry.byName.getValue(tool).providerIds)
        }
    }

    @Test
    fun targetToolsKeepDetectorAsGroundingOwnerAndEffectBackendsAsDependencies() {
        val expectedDependencies = setOf(
            "accessibility",
            "sgt_adb_bridge",
            "shizuku_shell",
            "root_bridge",
        )

        listOf("click_target", "click_mark", "drag_target").forEach { tool ->
            val spec = PhoneControlToolRegistry.byName.getValue(tool)
            assertEquals(listOf("local_ui_detector"), spec.providerIds)
            assertEquals(expectedDependencies, spec.dependencyProviderIds)
        }
    }

    @Test
    fun semanticActNeverPromotesAdbToItsGroundingProvider() {
        val click = PhoneControlToolRegistry.resolve(
            "act",
            buildJsonObject { put("verb", "click") },
        )
        val fill = PhoneControlToolRegistry.resolve(
            "act",
            buildJsonObject { put("verb", "fill") },
        )

        assertEquals(listOf("accessibility"), click?.providerIds)
        assertEquals("ui.pointer_action", click?.capability)
        assertEquals(listOf("accessibility"), fill?.providerIds)
        assertEquals("ui.text_edit", fill?.capability)
        assertTrue(fill?.dependencyProviderIds.isNullOrEmpty())
    }
}
