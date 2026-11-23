package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityRequest
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderExecutionPlanDecision
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderPlanRejection
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderReceiptDecision
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ProviderRouterTest {
    @Test
    fun `execution plan constrains a composite handler without readiness pre-gating`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)
        val request = CapabilityRequest("command_execution", "run_command")

        val planned = router.executionPlan(
            request,
            listOf("shizuku_shell", "root_bridge"),
        ) as ProviderExecutionPlanDecision.Planned
        val accepted = router.attestReceipt(planned.plan, "root_bridge")
            as ProviderReceiptDecision.Accepted

        assertEquals(listOf("shizuku_shell", "root_bridge"), planned.plan.providers.map { it.id })
        assertEquals("root_bridge", accepted.provider.id)
        assertEquals(1, accepted.priorityIndex)
    }

    @Test
    fun `execution plan rejects providers outside capability and tool order`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)
        val request = CapabilityRequest("ui.text_edit", "type_text")

        val outside = router.executionPlan(request, listOf("android_app_api"))
            as ProviderExecutionPlanDecision.Invalid
        val reversed = router.executionPlan(
            request,
            listOf("accessibility_input_method", "accessibility"),
        ) as ProviderExecutionPlanDecision.Invalid

        assertEquals(ProviderPlanRejection.PROVIDER_OUTSIDE_CAPABILITY_ROUTE, outside.rejection)
        assertEquals(ProviderPlanRejection.PROVIDER_ORDER_MISMATCH, reversed.rejection)
    }

    @Test
    fun `test-only routing model stays deleted`() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val capability = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/capability",
        )
        val source = capability.listFiles().orEmpty()
            .filter { it.extension == "kt" }
            .joinToString("\n") { it.readText() }
        listOf(
            "fun route(",
            "ProviderRouteDecision",
            "RouteRejection",
            "RouteAttempt",
            "fun supplies(",
            "fun fromWireName(",
            "isReadyForRouting",
            "PhoneControlCapabilityContract",
            "capabilitiesByProvider",
        ).forEach { symbol -> assertFalse(symbol, source.contains(symbol)) }
    }
}
