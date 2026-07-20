package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class BrowserStatusContract(
    val code: String,
    val state: CapabilityState,
    val providerId: String,
    val providerRole: BrowserProviderRole,
    val retryable: Boolean,
    val requiredUserStep: String? = null,
    val freshObservationRequired: Boolean = false,
)

internal fun browserStatusContract(
    probe: BrowserBaselineProbe,
    surface: BrowserSurfaceResolution?,
    setup: Boolean,
): BrowserStatusContract = if (setup) {
    setupContract(probe, surface)
} else {
    statusContract(probe, surface)
}

private fun setupContract(
    probe: BrowserBaselineProbe,
    surface: BrowserSurfaceResolution?,
): BrowserStatusContract {
    if (!probe.customTabsReady) {
        return providerRequirement(
            providerId = BROWSER_CUSTOM_TABS_PROVIDER,
            providerRole = BrowserProviderRole.PRIMARY,
            requiredUserStep = "choose_custom_tabs_browser",
        )
    }
    if (!probe.accessibilityReady) {
        return providerRequirement(
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            providerRole = BrowserProviderRole.DEPENDENCY,
            requiredUserStep = "enable_accessibility",
        )
    }
    return when (surface) {
        is BrowserSurfaceResolution.Success,
        is BrowserSurfaceResolution.Failure -> when {
            surface is BrowserSurfaceResolution.Failure &&
                surface.kind == BrowserSurfaceFailureKind.PROVIDER ->
                surface.toStatusContract(BROWSER_CUSTOM_TABS_PROVIDER)
            else -> BrowserStatusContract(
                code = "ok",
                state = CapabilityState.READY,
                providerId = BROWSER_CUSTOM_TABS_PROVIDER,
                providerRole = BrowserProviderRole.PRIMARY,
                retryable = false,
            )
        }
        null -> BrowserStatusContract(
            code = "browser_surface_not_observed",
            state = CapabilityState.DEGRADED,
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            providerRole = BrowserProviderRole.DEPENDENCY,
            retryable = true,
            freshObservationRequired = true,
        )
    }
}

private fun statusContract(
    probe: BrowserBaselineProbe,
    surface: BrowserSurfaceResolution?,
): BrowserStatusContract {
    if (!probe.accessibilityReady) {
        return providerRequirement(
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            providerRole = BrowserProviderRole.PRIMARY,
            requiredUserStep = "enable_accessibility",
        )
    }
    return when (surface) {
        is BrowserSurfaceResolution.Success -> BrowserStatusContract(
            code = "ok",
            state = CapabilityState.READY,
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            providerRole = BrowserProviderRole.PRIMARY,
            retryable = false,
        )
        is BrowserSurfaceResolution.Failure ->
            surface.toStatusContract(BROWSER_ACCESSIBILITY_PROVIDER)
        null -> BrowserStatusContract(
            code = "browser_surface_not_observed",
            state = CapabilityState.DEGRADED,
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            providerRole = BrowserProviderRole.PRIMARY,
            retryable = true,
            freshObservationRequired = true,
        )
    }
}

private fun providerRequirement(
    providerId: String,
    providerRole: BrowserProviderRole,
    requiredUserStep: String,
) = BrowserStatusContract(
    code = "browser_setup_required",
    state = CapabilityState.NEEDS_USER_STEP,
    providerId = providerId,
    providerRole = providerRole,
    retryable = true,
    requiredUserStep = requiredUserStep,
)

private fun BrowserSurfaceResolution.Failure.toStatusContract(
    primaryProviderId: String,
): BrowserStatusContract = BrowserStatusContract(
    code = code,
    state = if (kind == BrowserSurfaceFailureKind.SURFACE_STATE) {
        CapabilityState.DEGRADED
    } else {
        providerState
    },
    providerId = providerId ?: primaryProviderId,
    providerRole = if (providerId == null || providerId == primaryProviderId) {
        BrowserProviderRole.PRIMARY
    } else {
        BrowserProviderRole.DEPENDENCY
    },
    retryable = retryable,
    requiredUserStep = requiredUserStep,
    freshObservationRequired = freshObservationRequired,
)

internal fun browserStatusData(
    probe: BrowserBaselineProbe,
    surface: BrowserSurfaceResolution?,
    setup: Boolean,
): JsonObject = buildJsonObject {
    put("baseline", buildJsonObject {
        put("custom_tabs_ready", probe.customTabsReady)
        put("preferred_browser_package", probe.preferredPackage.orEmpty())
        put("accessibility_ready", probe.accessibilityReady)
        put("credential_context", "preferred_browser")
        put("credential_state_exported", false)
        put("control_provider", BROWSER_ACCESSIBILITY_PROVIDER)
    })
    put("surface_status", surfaceStatusData(surface))
    put("browser_cdp", buildJsonObject {
        put("state", "unavailable")
        put("dom_authority", false)
        put("cookie_access", false)
        put("tab_id_authority", false)
    })
    put("supported_operations", buildJsonArray {
        add("persistent_navigation")
        add("visible_accessibility_text")
        add("verified_back_when_url_is_visible")
    })
    put("unsupported_without_cdp", buildJsonArray {
        add("dom_eval")
        add("network")
        add("console")
        add("upload")
        add("tab_ids")
        add("whole_page_completeness")
    })
    (surface as? BrowserSurfaceResolution.Success)?.let {
        put("surface", it.snapshot.binding.toWireJson())
    }
    if (setup) {
        put("guidance", buildJsonArray {
            if (!probe.customTabsReady) add("Choose a default browser with Custom Tabs support.")
            if (!probe.accessibilityReady) add("Enable SGT Accessibility for visible browser control.")
            add("CDP remains optional and is required only for deep page, tab, and DevTools tools.")
        })
    }
}

private fun surfaceStatusData(surface: BrowserSurfaceResolution?): JsonObject = buildJsonObject {
    when (surface) {
        is BrowserSurfaceResolution.Success -> {
            put("bound", true)
            put("state", CapabilityState.READY.wireName)
            put("provider", BROWSER_ACCESSIBILITY_PROVIDER)
            put("observation_generation", surface.snapshot.observation.generation)
        }
        is BrowserSurfaceResolution.Failure -> {
            put("bound", false)
            put("state", surface.providerState.wireName)
            put("code", surface.code)
            put("failure_kind", surface.kind.wireName)
            put("retryable", surface.retryable)
            put("observation_generation", surface.observationGeneration)
            surface.providerId?.let { put("provider", it) }
            surface.requiredUserStep?.let { put("required_user_step", it) }
            if (surface.freshObservationRequired) put("fresh_observation_required", true)
        }
        null -> {
            put("bound", false)
            put("state", "not_observed")
        }
    }
}
