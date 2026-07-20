package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityFixture
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BROWSER_ACCESSIBILITY_PROVIDER
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BROWSER_CUSTOM_TABS_PROVIDER
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserBaselineProbe
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserProviderOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserProviderRole
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserSurfaceFailureKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserSurfaceResolution
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserStatusContract
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.browserStatusContract
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.browserStatusData
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BrowserToolHandlersTest {
    @Test
    fun providerRefusalKeepsUserStepAndFreshObservationMetadata() {
        val execution = browserProviderExecution(
            job = JOB,
            requestedTool = "browser_history",
            capability = "browser_authenticated_navigation",
            result = BrowserProviderOutcome(
                code = "os_owned_confirmation",
                state = CapabilityState.NEEDS_USER_STEP,
                providerId = BROWSER_ACCESSIBILITY_PROVIDER,
                data = buildJsonObject {},
                observationGeneration = 9,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                retryable = true,
                requiredUserStep = "complete_os_owned_confirmation",
                freshObservationRequired = true,
            ),
        )

        assertEquals("os_owned_confirmation", execution.response.value("code"))
        assertEquals(
            "complete_os_owned_confirmation",
            execution.response.getValue("required_user_step").jsonObject.value("code"),
        )
        assertEquals("true", execution.response.value("fresh_observation_required"))
        assertEquals("proven_no_effect", execution.response.value("effect_status"))
        assertFalse(execution.mutating)
        assertFalse(execution.refreshScreenFrame)
    }

    @Test
    fun readAndExtractKeepUsablePartialCapturesAcrossTheDispatchBoundary() = runTest {
        val tools = mapOf(
            "browser_read_page" to partialCaptureData(includePreview = true),
            "browser_extract_page" to partialCaptureData(includePreview = false),
        )

        tools.forEach { (tool, data) ->
            val execution = dispatch(
                tool,
                BrowserProviderOutcome(
                    code = "partial_capture",
                    state = CapabilityState.DEGRADED,
                    providerId = BROWSER_ACCESSIBILITY_PROVIDER,
                    data = data,
                    observationGeneration = 12,
                    effect = EffectCertainty.PROVEN_NO_EFFECT,
                    snapshotInvalidated = false,
                ),
            )

            assertEquals("partial_capture", execution.response.value("code"))
            assertEquals("degraded", execution.response.value("provider_state"))
            assertEquals(BROWSER_ACCESSIBILITY_PROVIDER, execution.response.value("provider"))
            assertEquals("primary", execution.response.value("provider_role"))
            assertFalse(execution.response.toString().contains("provider_contract_failure"))
            val page = execution.response.getValue("page").jsonObject
            val artifact = execution.response.getValue("artifact").jsonObject
            if (tool == "browser_read_page") {
                assertTrue(page.containsKey("text"))
                assertTrue(artifact.containsKey("preview"))
            } else {
                assertFalse(page.containsKey("text"))
                assertFalse(artifact.containsKey("preview"))
            }
        }
    }

    @Test
    fun offBrowserStatusAndSetupRemainTruthfulAcrossTheDispatchBoundary() = runTest {
        val surface = BrowserSurfaceResolution.Failure(
            code = "browser_surface_not_visible",
            message = "No exact foreground browser surface is visible.",
            retryable = true,
            kind = BrowserSurfaceFailureKind.SURFACE_STATE,
            observationGeneration = 23,
        )
        val status = browserStatusContract(READY_BASELINE, surface, setup = false)
        val setup = browserStatusContract(READY_BASELINE, surface, setup = true)

        val statusExecution = dispatch(
            "browser_status",
            status.outcome(browserStatusData(READY_BASELINE, surface, setup = false)),
        )
        val setupExecution = dispatch(
            "browser_setup",
            setup.outcome(browserStatusData(READY_BASELINE, surface, setup = true)),
        )

        assertEquals("browser_surface_not_visible", statusExecution.response.value("code"))
        assertEquals("degraded", statusExecution.response.value("provider_state"))
        assertEquals(BROWSER_ACCESSIBILITY_PROVIDER, statusExecution.response.value("provider"))
        assertEquals("ok", setupExecution.response.value("code"))
        assertEquals("ready", setupExecution.response.value("provider_state"))
        assertEquals(BROWSER_CUSTOM_TABS_PROVIDER, setupExecution.response.value("provider"))
        listOf(statusExecution, setupExecution).forEach { execution ->
            assertFalse(execution.response.toString().contains("provider_contract_failure"))
            assertFalse(
                execution.response
                    .getValue("surface_status")
                    .jsonObject
                    .getValue("bound")
                    .jsonPrimitive
                    .content
                    .toBoolean(),
            )
        }
    }

    @Test
    fun setupProviderRaceReturnsAnAttestedDependencyFailure() = runTest {
        val lostAccessibility = BrowserSurfaceResolution.Failure(
            code = "capability_unavailable",
            message = "The Accessibility provider disconnected.",
            retryable = true,
            kind = BrowserSurfaceFailureKind.PROVIDER,
            observationGeneration = 29,
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            providerState = CapabilityState.NEEDS_USER_STEP,
            requiredUserStep = "enable_accessibility",
            freshObservationRequired = true,
        )
        val contract = browserStatusContract(READY_BASELINE, lostAccessibility, setup = true)

        val execution = dispatch(
            "browser_setup",
            contract.outcome(browserStatusData(READY_BASELINE, lostAccessibility, setup = true)),
        )

        assertEquals("capability_unavailable", execution.response.value("code"))
        assertEquals(BROWSER_ACCESSIBILITY_PROVIDER, execution.response.value("provider"))
        assertEquals("dependency", execution.response.value("provider_role"))
        assertEquals("needs_user_step", execution.response.value("provider_state"))
        assertEquals(
            "enable_accessibility",
            execution.response.getValue("required_user_step").jsonObject.value("code"),
        )
        assertFalse(execution.response.toString().contains("provider_contract_failure"))
    }

    private suspend fun dispatch(
        tool: String,
        outcome: BrowserProviderOutcome,
    ): PhoneControlToolExecution {
        val dispatcher = PhoneControlToolDispatcher(
            executor = PhoneControlHandlerExecutor { _, job, requestedTool, _ ->
                val capability = PhoneControlToolRegistry.byName.getValue(requestedTool).capability
                browserProviderExecution(job, requestedTool, capability, outcome)
            },
            providerRouter = ROUTER,
            failureReporter = PhoneControlToolFailureReporter { _, _, error -> throw error },
        )
        return dispatcher.dispatch(JOB, tool, buildJsonObject {})
    }

    private fun BrowserStatusContract.outcome(data: JsonObject) = BrowserProviderOutcome(
        code = code,
        state = state,
        providerId = providerId,
        providerRole = providerRole,
        data = data,
        observationGeneration = 23,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = retryable,
        requiredUserStep = requiredUserStep,
        freshObservationRequired = freshObservationRequired,
    )

    private fun partialCaptureData(includePreview: Boolean): JsonObject = buildJsonObject {
        put("page", buildJsonObject {
            put("capture_complete", false)
            if (includePreview) put("text", "safe visible text")
        })
        put("artifact", buildJsonObject {
            put("id", "safe-artifact")
            if (includePreview) put("preview", "safe visible text")
        })
    }

    private fun JsonObject.value(key: String): String = getValue(key).jsonPrimitive.content

    private companion object {
        val READY_BASELINE = BrowserBaselineProbe(
            customTabsPackages = setOf("browser.package"),
            preferredPackage = "browser.package",
            accessibilityReady = true,
        )
        val ROUTER = PhoneControlAuthorityFixture.load().let {
            ProviderRouter(it.providers, it.routes)
        }
        val JOB = PhoneControlToolJobContext(1, "browser-job", 2)
    }
}
