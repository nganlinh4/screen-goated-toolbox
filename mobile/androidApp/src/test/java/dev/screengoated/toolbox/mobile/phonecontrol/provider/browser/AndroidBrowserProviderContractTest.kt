package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidBrowserProviderContractTest {
    private val readyBaseline = BrowserBaselineProbe(
        customTabsPackages = setOf("browser.package"),
        preferredPackage = "browser.package",
        accessibilityReady = true,
    )
    private val invisibleSurface = BrowserSurfaceResolution.Failure(
        code = "browser_surface_not_visible",
        message = "No exact foreground browser surface is visible.",
        retryable = true,
        kind = BrowserSurfaceFailureKind.SURFACE_STATE,
        observationGeneration = 17,
    )

    @Test
    fun setupRequiresProvidersButNotAnAlreadyVisibleBrowserSurface() {
        val contract = browserStatusContract(readyBaseline, invisibleSurface, setup = true)

        assertEquals("ok", contract.code)
        assertEquals(CapabilityState.READY, contract.state)
        assertEquals(BROWSER_CUSTOM_TABS_PROVIDER, contract.providerId)
        assertEquals(BrowserProviderRole.PRIMARY, contract.providerRole)
        assertFalse(contract.retryable)
        assertNull(contract.requiredUserStep)
    }

    @Test
    fun statusPreservesTheTypedSurfaceFailure() {
        val contract = browserStatusContract(readyBaseline, invisibleSurface, setup = false)

        assertEquals("browser_surface_not_visible", contract.code)
        assertEquals(CapabilityState.DEGRADED, contract.state)
        assertEquals(BROWSER_ACCESSIBILITY_PROVIDER, contract.providerId)
        assertEquals(BrowserProviderRole.PRIMARY, contract.providerRole)
        assertTrue(contract.retryable)
        assertNull(contract.requiredUserStep)
    }

    @Test
    fun setupDoesNotHideAProviderFailureAfterItsBaselineProbe() {
        val lostAccessibility = providerFailure(
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            state = CapabilityState.NEEDS_USER_STEP,
            requiredUserStep = "enable_accessibility",
        )

        val contract = browserStatusContract(readyBaseline, lostAccessibility, setup = true)

        assertEquals("capability_unavailable", contract.code)
        assertEquals(CapabilityState.NEEDS_USER_STEP, contract.state)
        assertEquals(BROWSER_ACCESSIBILITY_PROVIDER, contract.providerId)
        assertEquals(BrowserProviderRole.DEPENDENCY, contract.providerRole)
        assertEquals("enable_accessibility", contract.requiredUserStep)
        assertTrue(contract.freshObservationRequired)
    }

    @Test
    fun statusKeepsItsPrimaryProviderIdentityAcrossTheSameRace() {
        val lostAccessibility = providerFailure(
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            state = CapabilityState.NEEDS_USER_STEP,
            requiredUserStep = "enable_accessibility",
        )

        val contract = browserStatusContract(readyBaseline, lostAccessibility, setup = false)

        assertEquals("capability_unavailable", contract.code)
        assertEquals(CapabilityState.NEEDS_USER_STEP, contract.state)
        assertEquals(BROWSER_ACCESSIBILITY_PROVIDER, contract.providerId)
        assertEquals(BrowserProviderRole.PRIMARY, contract.providerRole)
    }

    @Test
    fun statusDiagnosticsAreStructuredAndDoNotCopyProviderMessages() {
        val failure = providerFailure(
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            state = CapabilityState.DEGRADED,
            message = PROTECTED_CANARY,
        )

        val data = browserStatusData(readyBaseline, failure, setup = false)
        val surface = data.getValue("surface_status").jsonObject

        assertFalse(data.toString().contains(PROTECTED_CANARY))
        assertFalse(surface.getValue("bound").jsonPrimitive.boolean)
        assertEquals("provider", surface.text("failure_kind"))
        assertEquals("capability_unavailable", surface.text("code"))
        assertEquals(BROWSER_ACCESSIBILITY_PROVIDER, surface.text("provider"))
        assertEquals("degraded", surface.text("state"))
        assertEquals("19", surface.text("observation_generation"))
    }

    private fun providerFailure(
        providerId: String,
        state: CapabilityState,
        message: String = "Provider unavailable.",
        requiredUserStep: String? = null,
    ) = BrowserSurfaceResolution.Failure(
        code = "capability_unavailable",
        message = message,
        retryable = true,
        kind = BrowserSurfaceFailureKind.PROVIDER,
        observationGeneration = 19,
        providerId = providerId,
        providerState = state,
        requiredUserStep = requiredUserStep,
        freshObservationRequired = true,
    )

    private fun kotlinx.serialization.json.JsonObject.text(key: String): String =
        getValue(key).jsonPrimitive.content

    private companion object {
        const val PROTECTED_CANARY = "protected-provider-message-91f0"
    }
}
