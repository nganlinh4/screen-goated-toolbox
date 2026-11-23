package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.activeOsOwnedUserStepFailure
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult
import kotlinx.coroutines.CancellationException

/** Samples platform authority after provider readiness, immediately before command dispatch. */
internal suspend fun commandAuthorityPreflight(
    observe: suspend () -> AccessibilityProviderResult<AccessibilityObservation> = {
        PhoneControlAccessibilityProvider.observeForVisual()
    },
    currentGeneration: () -> Long = { PhoneControlAccessibilityProvider.observationGeneration },
): PrivilegedCommandResult.Failure? {
    val result = try {
        observe()
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (_: Exception) {
        return unavailableCommandAuthority()
    }
    val observation = when (result) {
        is AccessibilityProviderResult.Failure -> return unavailableCommandAuthority()
        is AccessibilityProviderResult.Success -> result.value
    }
    if (observation.generation != currentGeneration()) {
        return unavailableCommandAuthority()
    }
    observation.activeOsOwnedUserStepFailure(AccessibilityMutationKind.POINTER_ACTIVATE)
        ?.let { failure ->
            return PrivilegedCommandResult.Failure(
                code = failure.code,
                message = failure.message,
                state = CapabilityState.NEEDS_USER_STEP,
                requiredUserStep = failure.requiredUserStep,
                effectMayHaveOccurred = false,
                freshObservationRequired = failure.freshObservationRequired,
            )
        }
    val activeWindows = observation.windows.filter { (it.active || it.focused) && !it.controllerOwned }
    if (activeWindows.isEmpty() || activeWindows.any { it.packageName.isNullOrBlank() }) {
        return unavailableCommandAuthority()
    }
    return null
}

private fun unavailableCommandAuthority() = PrivilegedCommandResult.Failure(
    code = "authority_observation_unavailable",
    message = "The current Android surface could not be checked before command dispatch.",
    state = CapabilityState.DEGRADED,
    providerGuidance = "Restore Accessibility and observe the current surface before retrying.",
    effectMayHaveOccurred = false,
    freshObservationRequired = true,
)
