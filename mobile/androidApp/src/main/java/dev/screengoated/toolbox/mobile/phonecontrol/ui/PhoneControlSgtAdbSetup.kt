package dev.screengoated.toolbox.mobile.phonecontrol.ui

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbBridgeCondition
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbBridgeProbe

internal enum class PhoneControlSgtAdbSetupAction {
    COMPLETE,
    LEAVE_PENDING,
    RECONNECT,
    OPEN_SETTINGS,
}

internal data class PhoneControlSgtAdbSetupAttempt(
    val condition: SgtAdbBridgeCondition,
    val action: PhoneControlSgtAdbSetupAction,
)

internal enum class PhoneControlSgtAdbRepeatDisposition {
    DISPATCH,
    WAIT_FOR_RETURN,
    LEAVE_SELECTED_PENDING,
}

internal fun phoneControlSgtAdbSetupAttempt(
    probe: SgtAdbBridgeProbe,
): PhoneControlSgtAdbSetupAttempt = PhoneControlSgtAdbSetupAttempt(
    condition = probe.condition,
    action = when (probe.state) {
        CapabilityState.READY -> PhoneControlSgtAdbSetupAction.COMPLETE
        CapabilityState.UNSUPPORTED -> PhoneControlSgtAdbSetupAction.LEAVE_PENDING
        CapabilityState.DEGRADED -> PhoneControlSgtAdbSetupAction.RECONNECT
        CapabilityState.NEEDS_USER_STEP,
        CapabilityState.REVOKED,
        CapabilityState.UNAVAILABLE,
        -> PhoneControlSgtAdbSetupAction.OPEN_SETTINGS
    },
)

internal fun phoneControlSgtAdbRepeatDisposition(
    attempt: PhoneControlSgtAdbSetupAttempt,
    previous: PhoneControlSgtAdbSetupAttempt?,
    stepActive: Boolean,
): PhoneControlSgtAdbRepeatDisposition = when {
    attempt != previous -> PhoneControlSgtAdbRepeatDisposition.DISPATCH
    stepActive -> PhoneControlSgtAdbRepeatDisposition.WAIT_FOR_RETURN
    else -> PhoneControlSgtAdbRepeatDisposition.LEAVE_SELECTED_PENDING
}
