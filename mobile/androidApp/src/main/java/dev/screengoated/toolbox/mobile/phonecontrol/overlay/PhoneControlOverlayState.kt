package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlServiceState
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase

internal data class PhoneControlOverlayVisual(
    val stateLabel: String,
    val iconOverride: String?,
    val caption: String,
    val listeningLevel: Float,
    val visible: Boolean,
)

internal fun phoneControlOverlayVisual(
    state: PhoneControlServiceState,
): PhoneControlOverlayVisual {
    if (!state.running || state.phase in HIDDEN_PHASES) {
        return PhoneControlOverlayVisual(
            GeneratedPhoneControlContract.ORB_STATE_IDLE,
            null,
            "",
            0f,
            false,
        )
    }
    val stateLabel = when (state.phase) {
        PhoneControlRuntimePhase.STARTING,
        PhoneControlRuntimePhase.CONNECTING,
        PhoneControlRuntimePhase.RECONNECTING,
        -> GeneratedPhoneControlContract.ORB_STATE_THINKING
        PhoneControlRuntimePhase.LISTENING -> GeneratedPhoneControlContract.ORB_STATE_IDLE
        PhoneControlRuntimePhase.WORKING,
        PhoneControlRuntimePhase.FINALIZING,
        -> state.orbStateLabel
        PhoneControlRuntimePhase.DEGRADED -> GeneratedPhoneControlContract.ORB_STATE_ERROR
        PhoneControlRuntimePhase.ERROR,
        PhoneControlRuntimePhase.STOPPED,
        -> GeneratedPhoneControlContract.ORB_STATE_IDLE
    }
    val caption = when (state.phase) {
        PhoneControlRuntimePhase.LISTENING -> ""
        PhoneControlRuntimePhase.WORKING -> state.outputCaption.ifBlank { state.inputCaption }
        PhoneControlRuntimePhase.FINALIZING -> state.outputCaption
        PhoneControlRuntimePhase.STARTING,
        PhoneControlRuntimePhase.CONNECTING,
        PhoneControlRuntimePhase.RECONNECTING,
        PhoneControlRuntimePhase.DEGRADED,
        -> state.userMessage
        PhoneControlRuntimePhase.ERROR,
        PhoneControlRuntimePhase.STOPPED,
        -> ""
    }
    return PhoneControlOverlayVisual(
        stateLabel = stateLabel,
        iconOverride = state.orbIconOverride,
        caption = caption,
        listeningLevel = state.listeningLevel.coerceIn(0f, 1f),
        visible = true,
    )
}

private val HIDDEN_PHASES = setOf(
    PhoneControlRuntimePhase.ERROR,
    PhoneControlRuntimePhase.STOPPED,
)
