package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase

internal data class PhoneControlServiceState(
    val running: Boolean,
    val phase: PhoneControlRuntimePhase,
    val code: PhoneControlRuntimeCode,
    val userMessage: String,
    val inputCaption: String = "",
    val outputCaption: String = "",
    val listeningLevel: Float = 0f,
    val orbStateLabel: String = GeneratedPhoneControlContract.ORB_STATE_IDLE,
    val orbIconOverride: String? = null,
    val authorityGuidance: String = "",
)

internal fun interface PhoneControlOverlayStateSink {
    fun onState(state: PhoneControlServiceState)
}

internal fun PhoneControlServiceState.notificationMessage(): String =
    authorityGuidance.ifBlank { userMessage }

internal fun stoppedPhoneControlServiceState() = PhoneControlServiceState(
    running = false,
    phase = PhoneControlRuntimePhase.STOPPED,
    code = PhoneControlRuntimeCode.STOPPED,
    userMessage = "",
)
