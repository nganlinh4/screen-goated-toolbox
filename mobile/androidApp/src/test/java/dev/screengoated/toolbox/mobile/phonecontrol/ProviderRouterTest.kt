package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityRequest
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouteDecision
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderExecutionPlanDecision
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderPlanRejection
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderReceiptDecision
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.capability.RouteRejection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProviderRouterTest {
    @Test
    fun `only ready providers are routeable across every fixture capability state`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)

        fixture.routes.forEach { route ->
            fixture.capabilityStates.forEach { stateName ->
                val state = CapabilityState.fromWireName(stateName)
                val request = CapabilityRequest(
                    capability = route.capability,
                    requestedTool = "tool_for_${route.capability}",
                )
                val snapshots = route.providerIds.map { providerId ->
                    snapshot(providerId, route.capability, state = state)
                }

                val decision = router.route(request, snapshots)

                if (state == CapabilityState.READY) {
                    val selected = decision as ProviderRouteDecision.Selected
                    assertEquals(route.providerIds.first(), selected.provider.id)
                } else {
                    val unavailable = decision as ProviderRouteDecision.Unavailable
                    assertEquals(route.providerIds.size, unavailable.attempts.size)
                    assertTrue(
                        unavailable.attempts.all {
                            it.state == state &&
                                it.rejection == RouteRejection.PROVIDER_NOT_READY
                        },
                    )
                }
            }
        }
    }

    @Test
    fun `every fixture route selects its first ready exact provider`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)

        fixture.routes.forEach { route ->
            val request = CapabilityRequest(
                capability = route.capability,
                requestedTool = "tool_for_${route.capability}",
            )
            val snapshots = route.providerIds.mapIndexed { index, providerId ->
                snapshot(
                    providerId = providerId,
                    capability = route.capability,
                    timestamp = index.toLong(),
                )
            }
            val selected = router.route(request, snapshots) as ProviderRouteDecision.Selected
            assertEquals(route.providerIds.first(), selected.provider.id)
            assertEquals(0, selected.priorityIndex)
            assertEquals(request.requestedTool, selected.request.requestedTool)
        }
    }

    @Test
    fun `router skips partial semantics and selects next complete provider`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)
        val request = CapabilityRequest(
            capability = "ui.pointer_action",
            requestedTool = "click_target",
            requiredSemantics = setOf("fresh_target", "verified_dispatch"),
        )
        val snapshots = listOf(
            snapshot(
                "owned_webview_bridge",
                request.capability,
                semantics = setOf("fresh_target"),
            ),
            snapshot("browser_cdp", "browser_semantic"),
            snapshot(
                "accessibility",
                request.capability,
                semantics = request.requiredSemantics,
            ),
            snapshot(
                "root_bridge",
                request.capability,
                semantics = request.requiredSemantics,
            ),
        )

        val selected = router.route(request, snapshots) as ProviderRouteDecision.Selected

        assertEquals("accessibility", selected.provider.id)
        assertEquals(2, selected.priorityIndex)
        assertEquals("click_target", selected.request.requestedTool)
    }

    @Test
    fun `stronger authority never overrides earlier ready evidence`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)
        val request = CapabilityRequest("app_and_task_control", "launch_app")
        val snapshots = listOf(
            snapshot("android_app_api", request.capability),
            snapshot("accessibility", request.capability),
            snapshot("shizuku_shell", request.capability),
            snapshot("root_bridge", request.capability),
            snapshot("privileged_system", request.capability),
        )

        val selected = router.route(request, snapshots) as ProviderRouteDecision.Selected

        assertEquals("android_app_api", selected.provider.id)
        assertEquals("app", selected.provider.authority)
    }

    @Test
    fun `non-ready providers produce typed unavailability without changing the tool`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)
        val request = CapabilityRequest("command_execution", "run_command")
        val snapshots = listOf(
            snapshot(
                "shizuku_shell",
                request.capability,
                state = CapabilityState.NEEDS_USER_STEP,
                requiredUserStep = "restart_shizuku_after_reboot",
            ),
            snapshot(
                "root_bridge",
                request.capability,
                state = CapabilityState.DEGRADED,
            ),
            snapshot(
                "privileged_system",
                request.capability,
                state = CapabilityState.UNAVAILABLE,
            ),
        )

        val unavailable = router.route(request, snapshots) as ProviderRouteDecision.Unavailable

        assertEquals("capability_unavailable", unavailable.code)
        assertEquals("command_execution", unavailable.request.capability)
        assertEquals("run_command", unavailable.request.requestedTool)
        assertEquals(3, unavailable.attempts.size)
        assertTrue(unavailable.attempts.all { it.rejection == RouteRejection.PROVIDER_NOT_READY })
        assertEquals(
            "restart_shizuku_after_reboot",
            unavailable.attempts.first().requiredUserStep,
        )
    }

    @Test
    fun `unknown capability cannot silently reroute to an advertised capability`() {
        val fixture = PhoneControlAuthorityFixture.load()
        val router = ProviderRouter(fixture.providers, fixture.routes)
        val request = CapabilityRequest("future.exact_capability", "future_tool")
        val snapshots = listOf(snapshot("accessibility", "ui.pointer_action"))

        val unavailable = router.route(request, snapshots) as ProviderRouteDecision.Unavailable

        assertEquals(request, unavailable.request)
        assertTrue(unavailable.attempts.isEmpty())
    }

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

    private fun snapshot(
        providerId: String,
        capability: String,
        state: CapabilityState = CapabilityState.READY,
        semantics: Set<String> = emptySet(),
        timestamp: Long = 1,
        requiredUserStep: String? = null,
    ) = ProviderSnapshot(
        providerId = providerId,
        state = state,
        supportedCapabilities = mapOf(capability to semantics),
        evidenceTimestampMs = timestamp,
        requiredUserStep = requiredUserStep,
    )
}
