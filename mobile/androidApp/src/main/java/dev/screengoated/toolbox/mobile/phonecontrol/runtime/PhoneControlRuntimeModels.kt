package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract

internal enum class PhoneControlRuntimePhase {
    STOPPED,
    STARTING,
    CONNECTING,
    LISTENING,
    WORKING,
    FINALIZING,
    RECONNECTING,
    DEGRADED,
    ERROR,
}

internal enum class PhoneControlRuntimeCode {
    STOPPED,
    STARTING,
    CONNECTING,
    READY,
    WORKING,
    FINALIZING,
    RECONNECTING,
    ACCESSIBILITY_UNAVAILABLE,
    SCREEN_CAPTURE_FAILED,
    TOOL_RECONCILIATION_REQUIRED,
    API_KEY_REQUIRED,
    CONFIGURATION_FAILED,
    MICROPHONE_FAILED,
    TRANSPORT_FAILED,
    RUNTIME_FAILED,
}

internal data class PhoneControlRuntimeSnapshot(
    val running: Boolean,
    val phase: PhoneControlRuntimePhase,
    val code: PhoneControlRuntimeCode,
    val message: String,
    val inputCaption: String = "",
    val outputCaption: String = "",
    val listeningLevel: Float = 0f,
    val orbStateLabel: String = GeneratedPhoneControlContract.ORB_STATE_IDLE,
    val orbIconOverride: String? = null,
) {
    init {
        require(listeningLevel in 0f..1f) { "listeningLevel must be between 0 and 1" }
    }

    companion object {
        fun stopped(message: String = "Phone Control is stopped.") = PhoneControlRuntimeSnapshot(
            running = false,
            phase = PhoneControlRuntimePhase.STOPPED,
            code = PhoneControlRuntimeCode.STOPPED,
            message = message,
        )
    }
}

internal fun interface PhoneControlRuntimeObserver {
    fun onSnapshot(snapshot: PhoneControlRuntimeSnapshot)
}
