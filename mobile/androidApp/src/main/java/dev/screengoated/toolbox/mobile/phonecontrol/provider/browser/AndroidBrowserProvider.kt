package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import android.accessibilityservice.AccessibilityService
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.browser.customtabs.CustomTabsIntent
import androidx.browser.customtabs.CustomTabsService
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStore
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.net.URI
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class BrowserProviderOutcome(
    val code: String,
    val state: CapabilityState,
    val providerId: String,
    val providerRole: BrowserProviderRole = BrowserProviderRole.PRIMARY,
    val data: JsonObject,
    val observationGeneration: Long,
    val effect: EffectCertainty,
    val snapshotInvalidated: Boolean,
    val retryable: Boolean = false,
    val requiredUserStep: String? = null,
    val freshObservationRequired: Boolean = false,
)

internal enum class BrowserProviderRole(val wireName: String) {
    PRIMARY("primary"),
    DEPENDENCY("dependency"),
}

internal data class BrowserBaselineProbe(
    val customTabsPackages: Set<String>,
    val preferredPackage: String?,
    val accessibilityReady: Boolean,
) {
    val customTabsReady: Boolean = preferredPackage != null
    val baselineReady: Boolean = customTabsReady && accessibilityReady
}

internal class AndroidBrowserProvider(
    private val context: Context,
    private val artifacts: PhoneControlArtifactStore,
) {
    private val lock = Any()
    private var binding: BrowserSurfaceBinding? = null

    fun probe(): BrowserBaselineProbe {
        val packages = customTabsPackages(context.packageManager)
        val defaultPackage = defaultBrowserPackage(context.packageManager)
        val preferred = defaultPackage?.takeIf(packages::contains)
            ?: packages.singleOrNull()
        return BrowserBaselineProbe(
            customTabsPackages = packages,
            preferredPackage = preferred,
            accessibilityReady = PhoneControlAccessibilityProvider.isReady,
        )
    }

    suspend fun status(setup: Boolean): BrowserProviderOutcome {
        val probe = probe()
        val surface = if (probe.accessibilityReady) currentSurface(probe) else null
        val contract = browserStatusContract(probe, surface, setup)
        return BrowserProviderOutcome(
            code = contract.code,
            state = contract.state,
            providerId = contract.providerId,
            providerRole = contract.providerRole,
            data = browserStatusData(probe, surface, setup),
            observationGeneration = when (surface) {
                is BrowserSurfaceResolution.Success -> surface.snapshot.observation.generation
                is BrowserSurfaceResolution.Failure -> surface.observationGeneration
                null -> 0
            },
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            retryable = contract.retryable,
            requiredUserStep = contract.requiredUserStep,
            freshObservationRequired = contract.freshObservationRequired,
        )
    }

    suspend fun navigate(url: String, lifetime: String): BrowserProviderOutcome {
        if (lifetime != "persistent") {
            return failure(
                code = "unsupported_tab_lifetime",
                state = CapabilityState.UNSUPPORTED,
                message = "Accessibility cannot prove turn-scoped tab ownership or cleanup.",
                providerId = BROWSER_CUSTOM_TABS_PROVIDER,
            )
        }
        val uri = parseHttpUri(url) ?: return failure(
            code = "invalid_url",
            state = CapabilityState.READY,
            message = "browser_navigate requires an absolute http or https URL.",
            providerId = BROWSER_CUSTOM_TABS_PROVIDER,
        )
        val probe = probe()
        val packageName = probe.preferredPackage ?: return failure(
            code = "capability_unavailable",
            state = CapabilityState.NEEDS_USER_STEP,
            message = "No preferred browser with Custom Tabs support is available.",
            providerId = BROWSER_CUSTOM_TABS_PROVIDER,
            requiredUserStep = "choose_custom_tabs_browser",
            retryable = true,
        )
        if (!probe.accessibilityReady) {
            return failure(
                code = "capability_unavailable",
                state = CapabilityState.NEEDS_USER_STEP,
                message = "Accessibility must be connected before binding a browser surface.",
                providerId = BROWSER_ACCESSIBILITY_PROVIDER,
                providerRole = BrowserProviderRole.DEPENDENCY,
                requiredUserStep = "enable_accessibility",
                retryable = true,
            )
        }
        val customTab = CustomTabsIntent.Builder().build().also {
            it.intent.setPackage(packageName)
            it.intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        val dispatched = runCatching { customTab.launchUrl(context, Uri.parse(uri.toString())) }
        if (dispatched.isFailure) {
            return failure(
                code = "navigation_not_dispatched",
                state = CapabilityState.DEGRADED,
                message = dispatched.exceptionOrNull()?.message ?: "The browser rejected navigation.",
                providerId = BROWSER_CUSTOM_TABS_PROVIDER,
                retryable = true,
            )
        }
        val surface = awaitSurface(probe, packageName, launchedByPhoneControl = true)
        if (surface !is BrowserSurfaceResolution.Success) {
            return BrowserProviderOutcome(
                code = "navigation_postcondition_unverified",
                state = CapabilityState.DEGRADED,
                providerId = BROWSER_CUSTOM_TABS_PROVIDER,
                data = buildJsonObject {
                    put("requested_url", uri.toString())
                    put("lifetime", lifetime)
                    put("dispatch", "accepted")
                    put("verification", surface.toFailureJson())
                    put("provider_metadata", providerMetadata("custom_tabs_session"))
                },
                observationGeneration = PhoneControlAccessibilityProvider.observationGeneration,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
                retryable = true,
            )
        }
        val snapshot = surface.snapshot
        val verified = snapshot.observedUrl?.let(::parseHttpUri) == uri
        return BrowserProviderOutcome(
            code = if (verified) "ok" else "navigation_postcondition_unverified",
            state = if (verified) CapabilityState.READY else CapabilityState.DEGRADED,
            providerId = BROWSER_CUSTOM_TABS_PROVIDER,
            data = buildJsonObject {
                put("requested_url", uri.toString())
                put("lifetime", lifetime)
                put("dispatch", "accepted")
                snapshot.observedUrl?.let { put("observed_url", it) }
                put("surface", snapshot.binding.toWireJson())
                put("provider_metadata", providerMetadata("custom_tabs_session"))
                put("verification", if (verified) "visible_url_matches" else "surface_only")
            },
            observationGeneration = snapshot.observation.generation,
            effect = if (verified) EffectCertainty.VERIFIED else EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            retryable = !verified,
        )
    }

    suspend fun capture(includePreview: Boolean): BrowserProviderOutcome {
        val probe = probe()
        if (!probe.accessibilityReady) {
            return failure(
                code = "capability_unavailable",
                state = CapabilityState.NEEDS_USER_STEP,
                message = "Accessibility is required for visible browser text.",
                requiredUserStep = "enable_accessibility",
                retryable = true,
            )
        }
        val snapshot = when (val resolved = currentSurface(probe)) {
            is BrowserSurfaceResolution.Success -> resolved.snapshot
            is BrowserSurfaceResolution.Failure ->
                return resolutionFailure(resolved, BROWSER_ACCESSIBILITY_PROVIDER)
        }
        val capture = captureVisibleBrowserText(snapshot.elements)
        val artifact = artifacts.put(
            bytes = capture.text.toByteArray(Charsets.UTF_8),
            mimeType = "text/plain; charset=utf-8",
            name = "browser-visible-accessibility-text.txt",
        )
        val presentation = buildBrowserCapturePresentation(
            title = snapshot.binding.windowTitle.orEmpty(),
            observedUrl = snapshot.observedUrl,
            observationTruncated = snapshot.observation.truncated,
            capture = capture,
            artifactInfo = artifact.info(),
            captureSha256 = artifact.sha256,
            includePreview = includePreview,
        )
        return BrowserProviderOutcome(
            code = "partial_capture",
            state = CapabilityState.DEGRADED,
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            data = buildJsonObject {
                put("page", presentation.page)
                put("artifact", presentation.artifact)
                put("surface", snapshot.binding.toWireJson())
                put("provider_metadata", providerMetadata("accessibility"))
                put(
                    "instruction",
                    "This is bounded visible Accessibility text, not DOM or whole-page proof.",
                )
                put("completion_proof", presentation.completionProof)
            },
            observationGeneration = snapshot.observation.generation,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
        )
    }

    suspend fun history(direction: String): BrowserProviderOutcome {
        if (direction !in setOf("back", "forward")) {
            return failure("invalid_direction", CapabilityState.READY, "direction must be back or forward.")
        }
        if (direction == "forward") {
            return failure(
                code = "capability_unavailable",
                state = CapabilityState.UNAVAILABLE,
                message = "Forward history requires an exact browser integration or CDP target.",
                requiredUserStep = "configure_browser_control",
            )
        }
        val probe = probe()
        val before = when (val beforeResult = currentSurface(probe)) {
            is BrowserSurfaceResolution.Success -> beforeResult.snapshot
            is BrowserSurfaceResolution.Failure ->
                return resolutionFailure(beforeResult, BROWSER_ACCESSIBILITY_PROVIDER)
        }
        val beforeUrl = before.observedUrl ?: return failure(
            code = "history_precondition_unverified",
            state = CapabilityState.DEGRADED,
            message = "The exact visible browser surface exposes no unique current URL.",
            retryable = true,
        )
        val sourceWindow = before.observation.windows.singleOrNull { window ->
            window.displayId == before.binding.displayId &&
                window.id.toLong() == before.binding.windowId &&
                window.packageName == before.binding.packageName
        } ?: return failure(
            code = "stale_target",
            state = CapabilityState.DEGRADED,
            message = "The bound browser surface no longer exists.",
            retryable = true,
        )
        val mutationLease = sourceWindow.surfaceLease(before.observation.generation)
            ?: return failure(
                code = "surface_authority_unknown",
                state = CapabilityState.DEGRADED,
                message = "The browser surface has no platform mutation authority.",
                retryable = true,
            )
        val dispatch = PhoneControlAccessibilityProvider.globalAction(
            mutationLease,
            AccessibilityService.GLOBAL_ACTION_BACK,
        )
        val outcome = when (dispatch) {
            is AccessibilityProviderResult.Success -> dispatch.value
            is AccessibilityProviderResult.Failure -> return failure(
                code = dispatch.code,
                state = CapabilityState.DEGRADED,
                message = dispatch.message,
                retryable = dispatch.retryable,
                requiredUserStep = dispatch.requiredUserStep,
                freshObservationRequired = dispatch.freshObservationRequired,
                effect = dispatch.effect,
                observationGeneration = before.observation.generation,
            )
        }
        if (outcome.effect == EffectCertainty.PROVEN_NO_EFFECT) {
            return failure("history_not_dispatched", CapabilityState.READY, "Android rejected Back.")
        }
        val afterResult = awaitSurface(probe, before.binding.packageName, launchedByPhoneControl = false)
        val after = (afterResult as? BrowserSurfaceResolution.Success)?.snapshot
        val verified = after != null &&
            after.binding.surfaceId == before.binding.surfaceId &&
            after.observedUrl != null &&
            after.observedUrl != beforeUrl
        return BrowserProviderOutcome(
            code = if (verified) "ok" else "history_postcondition_unverified",
            state = if (verified) CapabilityState.READY else CapabilityState.DEGRADED,
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            data = buildJsonObject {
                put("direction", direction)
                put("before_url", beforeUrl)
                after?.observedUrl?.let { put("observed_url", it) }
                put("before_surface", before.binding.toWireJson())
                after?.let { put("surface", it.binding.toWireJson()) }
                put("provider_metadata", providerMetadata("accessibility"))
            },
            observationGeneration = after?.observation?.generation ?: outcome.generation,
            effect = if (verified) EffectCertainty.VERIFIED else EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            retryable = !verified,
        )
    }

    private suspend fun currentSurface(probe: BrowserBaselineProbe): BrowserSurfaceResolution {
        val observation = PhoneControlAccessibilityProvider.observe(MAX_BROWSER_ELEMENTS)
        if (observation is AccessibilityProviderResult.Failure) {
            return accessibilityResolutionFailure(observation)
        }
        val value = (observation as AccessibilityProviderResult.Success).value
        val previous = synchronized(lock) { binding }
        val resolved = resolveVisibleBrowserSurface(value, probe.customTabsPackages, previous)
        if (resolved is BrowserSurfaceResolution.Success) {
            synchronized(lock) { binding = resolved.snapshot.binding }
        }
        return resolved
    }

    private suspend fun awaitSurface(
        probe: BrowserBaselineProbe,
        packageName: String,
        launchedByPhoneControl: Boolean,
    ): BrowserSurfaceResolution {
        var last: BrowserSurfaceResolution = BrowserSurfaceResolution.Failure(
            "browser_surface_not_visible",
            "The launched browser surface is not visible yet.",
            retryable = true,
            kind = BrowserSurfaceFailureKind.SURFACE_STATE,
            observationGeneration = PhoneControlAccessibilityProvider.observationGeneration,
        )
        repeat(SURFACE_VERIFY_ATTEMPTS) { attempt ->
            if (attempt > 0) delay(SURFACE_VERIFY_DELAY_MS)
            val observation = PhoneControlAccessibilityProvider.observe(MAX_BROWSER_ELEMENTS)
            if (observation is AccessibilityProviderResult.Success) {
                val previous = synchronized(lock) { binding }
                last = resolveVisibleBrowserSurface(
                    observation = observation.value,
                    browserPackages = probe.customTabsPackages,
                    previous = previous,
                    preferredPackage = packageName,
                    launchedByPhoneControl = launchedByPhoneControl,
                )
                if (last is BrowserSurfaceResolution.Success) {
                    synchronized(lock) { binding = (last as BrowserSurfaceResolution.Success).snapshot.binding }
                    return last
                }
            } else {
                val failure = observation as AccessibilityProviderResult.Failure
                last = accessibilityResolutionFailure(failure)
            }
        }
        return last
    }

    private fun resolutionFailure(
        failure: BrowserSurfaceResolution.Failure,
        primaryProviderId: String,
    ): BrowserProviderOutcome {
        val providerId = failure.providerId ?: primaryProviderId
        return BrowserProviderOutcome(
            code = failure.code,
            state = if (failure.kind == BrowserSurfaceFailureKind.SURFACE_STATE) {
                CapabilityState.DEGRADED
            } else {
                failure.providerState
            },
            providerId = providerId,
            providerRole = if (providerId == primaryProviderId) {
                BrowserProviderRole.PRIMARY
            } else {
                BrowserProviderRole.DEPENDENCY
            },
            data = failure.safeDiagnosticData(),
            observationGeneration = failure.observationGeneration,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            retryable = failure.retryable,
            requiredUserStep = failure.requiredUserStep,
            freshObservationRequired = failure.freshObservationRequired,
        )
    }

    private fun accessibilityResolutionFailure(
        failure: AccessibilityProviderResult.Failure,
    ): BrowserSurfaceResolution.Failure {
        val providerReady = PhoneControlAccessibilityProvider.isReady
        return BrowserSurfaceResolution.Failure(
            code = failure.code,
            message = failure.message,
            retryable = failure.retryable,
            kind = BrowserSurfaceFailureKind.PROVIDER,
            observationGeneration = PhoneControlAccessibilityProvider.observationGeneration,
            providerId = BROWSER_ACCESSIBILITY_PROVIDER,
            providerState = if (providerReady && failure.requiredUserStep == null) {
                CapabilityState.DEGRADED
            } else {
                CapabilityState.NEEDS_USER_STEP
            },
            requiredUserStep = failure.requiredUserStep
                ?: if (providerReady) null else "enable_accessibility",
            freshObservationRequired = failure.freshObservationRequired,
        )
    }

    private fun failure(
        code: String,
        state: CapabilityState,
        message: String,
        providerId: String = BROWSER_ACCESSIBILITY_PROVIDER,
        providerRole: BrowserProviderRole = BrowserProviderRole.PRIMARY,
        requiredUserStep: String? = null,
        retryable: Boolean = false,
        freshObservationRequired: Boolean = false,
        effect: EffectCertainty = EffectCertainty.PROVEN_NO_EFFECT,
        observationGeneration: Long = 0,
    ) = BrowserProviderOutcome(
        code = code,
        state = state,
        providerId = providerId,
        providerRole = providerRole,
        data = buildJsonObject { put("message", message) },
        observationGeneration = observationGeneration,
        effect = effect,
        snapshotInvalidated = false,
        retryable = retryable,
        requiredUserStep = requiredUserStep,
        freshObservationRequired = freshObservationRequired,
    )
}

private fun customTabsPackages(packageManager: PackageManager): Set<String> {
    val intent = Intent(CustomTabsService.ACTION_CUSTOM_TABS_CONNECTION)
    return queryIntentServices(packageManager, intent)
        .mapNotNull { it.serviceInfo?.packageName }
        .toSet()
}

private fun defaultBrowserPackage(packageManager: PackageManager): String? {
    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(BROWSER_PROBE_URL))
        .addCategory(Intent.CATEGORY_BROWSABLE)
    return resolveActivity(packageManager, intent)?.activityInfo?.packageName
}

private fun queryIntentServices(
    packageManager: PackageManager,
    intent: Intent,
) = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
    packageManager.queryIntentServices(intent, PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_ALL.toLong()))
} else {
    @Suppress("DEPRECATION")
    packageManager.queryIntentServices(intent, PackageManager.MATCH_ALL)
}

private fun resolveActivity(packageManager: PackageManager, intent: Intent) =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        packageManager.resolveActivity(
            intent,
            PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_DEFAULT_ONLY.toLong()),
        )
    } else {
        @Suppress("DEPRECATION")
        packageManager.resolveActivity(intent, PackageManager.MATCH_DEFAULT_ONLY)
    }

private fun parseHttpUri(raw: String): URI? = runCatching { URI(raw.trim()).normalize() }
    .getOrNull()
    ?.takeIf { uri ->
        uri.scheme?.lowercase() in setOf("http", "https") && !uri.host.isNullOrBlank()
    }

private fun providerMetadata(controlProvider: String): JsonObject = buildJsonObject {
    put("credential_context_kind", "preferred_browser_shared_state")
    put("credential_continuity_provider", "custom_tabs_session")
    put("control_provider", controlProvider)
    put("credential_continuity_and_control_authority_are_independent", true)
    put("dom_authority", false)
    put("cookie_access", false)
    put("capture_scope", "visible_accessibility_nodes")
    put("capture_complete", false)
}

private fun BrowserSurfaceResolution?.toFailureJson(): JsonObject = buildJsonObject {
    val failure = this@toFailureJson as? BrowserSurfaceResolution.Failure
    put("status", "inconclusive")
    put("code", failure?.code ?: "browser_surface_not_visible")
    failure?.let {
        put("failure_kind", it.kind.wireName)
        put("retryable", it.retryable)
        put("observation_generation", it.observationGeneration)
        it.providerId?.let { providerId -> put("provider", providerId) }
        put("provider_state", it.providerState.wireName)
        it.requiredUserStep?.let { step -> put("required_user_step", step) }
    }
}

private fun BrowserSurfaceResolution.Failure.safeDiagnosticData(): JsonObject = buildJsonObject {
    put("failure_kind", kind.wireName)
    put("retryable", retryable)
    put("observation_generation", observationGeneration)
    providerId?.let { put("provider", it) }
    put("provider_state", providerState.wireName)
    requiredUserStep?.let { put("required_user_step", it) }
    if (freshObservationRequired) put("fresh_observation_required", true)
}

private const val BROWSER_PROBE_URL = "https://example.invalid/"
private const val MAX_BROWSER_ELEMENTS = 1_000
private const val SURFACE_VERIFY_ATTEMPTS = 8
private const val SURFACE_VERIFY_DELAY_MS = 180L
