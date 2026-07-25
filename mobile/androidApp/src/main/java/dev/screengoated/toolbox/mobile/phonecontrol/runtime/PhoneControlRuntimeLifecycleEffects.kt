package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import java.util.concurrent.atomic.AtomicBoolean

internal class PhoneControlRuntimeLifecycleEffects(
    private val transportReady: AtomicBoolean,
    private val statusPublisher: PhoneControlRuntimeStatusPublisher,
    private val prepareReconnect: () -> Boolean,
    private val abandonProtocolSession: () -> Unit,
    private val purgeSessionOutbound: () -> Unit,
    private val discardUntilFreshConnection: AtomicBoolean,
) {
    fun observe(effect: GeminiLiveLifecycleEffect) {
        when (effect) {
            is GeminiLiveLifecycleEffect.OpenSocket -> openSocket(effect)
            is GeminiLiveLifecycleEffect.SendSetup -> statusPublisher.publish(
                phase = PhoneControlRuntimePhase.STARTING,
                code = PhoneControlRuntimeCode.STARTING,
                message = "Preparing the Phone Control agent…",
            )
            is GeminiLiveLifecycleEffect.CloseSocket -> transportReady.set(false)
            is GeminiLiveLifecycleEffect.ScheduleReconnect -> reconnect(effect)
            is GeminiLiveLifecycleEffect.ReportFailure -> terminalFailure(effect)
            GeminiLiveLifecycleEffect.CancelSession -> transportReady.set(false)
            else -> Unit
        }
    }

    private fun openSocket(effect: GeminiLiveLifecycleEffect.OpenSocket) {
        transportReady.set(false)
        val reconnecting = effect.generation > 1L
        statusPublisher.publish(
            phase = if (reconnecting) {
                PhoneControlRuntimePhase.RECONNECTING
            } else {
                PhoneControlRuntimePhase.CONNECTING
            },
            code = if (reconnecting) {
                PhoneControlRuntimeCode.RECONNECTING
            } else {
                PhoneControlRuntimeCode.CONNECTING
            },
            message = if (reconnecting) {
                "Restoring the agent connection…"
            } else {
                "Connecting to Gemini Live…"
            },
        )
    }

    private fun reconnect(effect: GeminiLiveLifecycleEffect.ScheduleReconnect) {
        if (!prepareReconnect()) {
            discardUntilFreshConnection.set(true)
            abandonProtocolSession()
            purgeSessionOutbound()
        }
        Log.w(
            TAG,
            "transport_reconnect generation=${effect.generation} " +
                "attempt=${effect.attempt} reason=${effect.reason.fixtureName}",
        )
        statusPublisher.publish(
            phase = PhoneControlRuntimePhase.RECONNECTING,
            code = PhoneControlRuntimeCode.RECONNECTING,
            message = "Connection interrupted; retrying safely…",
        )
    }

    private fun terminalFailure(effect: GeminiLiveLifecycleEffect.ReportFailure) {
        Log.e(TAG, "transport_terminal_failure reason=${effect.reason}")
        statusPublisher.publish(
            running = false,
            phase = PhoneControlRuntimePhase.ERROR,
            code = PhoneControlRuntimeCode.TRANSPORT_FAILED,
            message = "Phone Control connection failed (${effect.reason}).",
        )
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
    }
}
