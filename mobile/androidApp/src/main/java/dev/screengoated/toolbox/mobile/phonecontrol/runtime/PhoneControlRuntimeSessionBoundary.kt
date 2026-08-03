package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleAdapter
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleConnection
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import java.util.concurrent.atomic.AtomicBoolean

internal class PhoneControlRuntimeSessionBoundary(
    private val lifecycle: GeminiLiveLifecycleAdapter,
    private val transportReady: AtomicBoolean,
    private val discardOutboundUntilFreshConnection: AtomicBoolean,
    private val setupSession: PhoneControlSetupSessionRuntime,
    private val userInterfaceGoals: PhoneControlUserInterfaceGoalQueue,
    private val turnCoordinator: PhoneControlTurnCoordinator,
    private val statusPublisher: PhoneControlRuntimeStatusPublisher,
    private val clearResumptionHandle: () -> Unit,
    private val purgeSessionOutbound: () -> Unit,
    private val requestScreenRefresh: () -> Unit,
) {
    fun bindReady(connection: GeminiLiveLifecycleConnection, becameReady: Boolean) {
        if (becameReady && discardOutboundUntilFreshConnection.compareAndSet(true, false)) {
            purgeSessionOutbound()
            turnCoordinator.freshProtocolSessionBound()
        }
        transportReady.set(true)
        if (!becameReady) return
        if (!setupSession.inputAdmitted) setupSession.observeFreshSession()
        Log.i(TAG, "transport_ready generation=${connection.generation}")
        requestScreenRefresh()
        statusPublisher.publishTurnPhase(turnCoordinator.phase)
    }

    suspend fun resetSetupConversation() {
        transportReady.set(false)
        clearResumptionHandle()
        discardOutboundUntilFreshConnection.set(true)
        userInterfaceGoals.clear()
        turnCoordinator.abandonProtocolSession()
        statusPublisher.clearConversation()
        purgeSessionOutbound()
        val generation = lifecycle.activeConnection?.generation ?: lifecycle.state.generation
        Log.i(
            TAG,
            "setup_session_state state=conversation_reset input_admitted=false generation=$generation",
        )
        failActiveTransport(generation)
    }

    suspend fun abortOverflowedProtocolSession(requested: AtomicBoolean): Boolean {
        if (!requested.compareAndSet(true, false)) return false
        transportReady.set(false)
        clearResumptionHandle()
        discardOutboundUntilFreshConnection.set(true)
        turnCoordinator.abandonProtocolSession()
        purgeSessionOutbound()
        val generation = lifecycle.activeConnection?.generation ?: lifecycle.state.generation
        Log.e(TAG, "protocol_overflow_abandon generation=$generation")
        failActiveTransport(generation)
        return true
    }

    private suspend fun failActiveTransport(generation: Long) {
        if (generation > 0L && lifecycle.state.phase != GeminiLiveLifecyclePhase.FAILED) {
            lifecycle.transportFailed(generation)
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
    }
}
