package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import kotlinx.serialization.json.JsonObject

internal sealed interface PrivilegedCommandResult {
    data class Success(val receipt: JsonObject) : PrivilegedCommandResult

    data class Failure(
        val code: String,
        val message: String,
        val state: CapabilityState,
        val providerGuidance: String? = null,
        val requiredUserStep: String? = null,
        val effectMayHaveOccurred: Boolean,
        val freshObservationRequired: Boolean = false,
    ) : PrivilegedCommandResult
}

internal fun AccessibilityProviderResult.Failure.toPrivilegedCommandFailure() =
    PrivilegedCommandResult.Failure(
        code = code,
        message = message,
        state = if (requiredUserStep != null) {
            CapabilityState.NEEDS_USER_STEP
        } else {
            CapabilityState.DEGRADED
        },
        requiredUserStep = requiredUserStep,
        effectMayHaveOccurred = effect.effectMayHaveOccurred == true,
        freshObservationRequired = freshObservationRequired,
    )
