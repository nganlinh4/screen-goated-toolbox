package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase

internal class PhoneControlRuntimeStatusPublisher(
    private val observer: PhoneControlRuntimeObserver,
    private val isTransportReady: () -> Boolean,
) {
    private val snapshotLock = Any()
    private var currentSnapshot = PhoneControlRuntimeSnapshot.stopped()

    fun updateListeningLevel(level: Float) {
        updateSnapshot { it.copy(listeningLevel = level.coerceIn(0f, 1f)) }
    }

    fun clearListeningLevel() {
        updateSnapshot { it.copy(listeningLevel = 0f) }
    }

    fun updateOrbPresentation(stateLabel: String, iconOverride: String?) {
        updateSnapshot {
            it.copy(orbStateLabel = stateLabel, orbIconOverride = iconOverride)
        }
    }

    fun updateCaption(
        input: String? = null,
        output: String? = null,
    ) {
        updateSnapshot { current ->
            current.copy(
                inputCaption = input?.takeLast(MAX_CAPTION_CHARACTERS) ?: current.inputCaption,
                outputCaption = output?.takeLast(MAX_CAPTION_CHARACTERS) ?: current.outputCaption,
            )
        }
    }

    fun publishTurnPhase(phase: PhoneControlTurnPhase) {
        if (!isTransportReady()) return
        when (phase) {
            PhoneControlTurnPhase.IDLE,
            PhoneControlTurnPhase.LISTENING,
            -> {
                updateOrbPresentation(GeneratedPhoneControlContract.ORB_STATE_IDLE, null)
                publish(
                    phase = PhoneControlRuntimePhase.LISTENING,
                    code = PhoneControlRuntimeCode.READY,
                    message = "Ready for a voice command.",
                )
            }
            PhoneControlTurnPhase.WORKING -> publish(
                phase = PhoneControlRuntimePhase.WORKING,
                code = PhoneControlRuntimeCode.WORKING,
                message = "Working on your request…",
            )
            PhoneControlTurnPhase.FINALIZING -> publish(
                phase = PhoneControlRuntimePhase.FINALIZING,
                code = PhoneControlRuntimeCode.FINALIZING,
                message = "Finishing the response…",
            )
        }
    }

    fun publishScreenFailure(message: String) {
        publish(
            phase = PhoneControlRuntimePhase.DEGRADED,
            code = PhoneControlRuntimeCode.SCREEN_CAPTURE_FAILED,
            message = message,
        )
    }

    fun publish(
        running: Boolean = true,
        phase: PhoneControlRuntimePhase,
        code: PhoneControlRuntimeCode,
        message: String,
    ) {
        updateSnapshot {
            it.copy(running = running, phase = phase, code = code, message = message)
        }
    }

    fun publishStopped() {
        updateSnapshot { PhoneControlRuntimeSnapshot.stopped() }
    }

    private fun updateSnapshot(
        transform: (PhoneControlRuntimeSnapshot) -> PhoneControlRuntimeSnapshot,
    ) {
        val next = synchronized(snapshotLock) {
            transform(currentSnapshot).also { currentSnapshot = it }
        }
        runCatching { observer.onSnapshot(next) }
            .onFailure { Log.e(TAG, "runtime_observer_failed", it) }
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
        const val MAX_CAPTION_CHARACTERS = 2_000
    }
}
