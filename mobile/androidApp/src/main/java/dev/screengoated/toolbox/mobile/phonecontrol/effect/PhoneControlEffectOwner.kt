package dev.screengoated.toolbox.mobile.phonecontrol.effect

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import java.nio.charset.StandardCharsets
import java.util.Base64
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.launch
import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

internal data class PhoneControlOperationId(
    val turnId: Long,
    val responseGeneration: Long,
    val jobId: String,
) {
    init {
        require(turnId > 0)
        require(responseGeneration > 0)
        require(jobId.isNotBlank())
    }

    val wireValue: String
        get() {
            val encodedJob = Base64.getUrlEncoder().withoutPadding()
                .encodeToString(jobId.toByteArray(StandardCharsets.UTF_8))
            return "pc1:$turnId:$responseGeneration:$encodedJob"
        }
}

/** Owns cancellation and terminal acknowledgement for one exact admitted tool job. */
internal class PhoneControlEffectOwner(
    val operationId: PhoneControlOperationId,
) : AbstractCoroutineContextElement(Key) {
    internal companion object Key : CoroutineContext.Key<PhoneControlEffectOwner>

    private val lock = Any()
    private var cancellationRequested = false
    private var effectBoundaryObserved = false
    private var effectMayHaveOccurred = false
    private var inFlightDispatchAttempts = 0
    private var pendingCancellationSettlements = 0
    private var nextHandlerId = 0L
    private var activeEffects = 0
    private var terminal = CompletableDeferred(Unit)
    private val cancellationHandlers = linkedMapOf<Long, suspend () -> Unit>()
    private val cancellationScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    val isCancellationRequested: Boolean
        get() = synchronized(lock) { cancellationRequested }

    fun requestCancellation(): PhoneControlEffectCertainty {
        val handlers: List<suspend () -> Unit>
        val certainty: PhoneControlEffectCertainty
        synchronized(lock) {
            handlers = if (cancellationRequested) {
                emptyList()
            } else {
                cancellationRequested = true
                cancellationHandlers.values.toList().also { pending ->
                    if (pending.isNotEmpty()) {
                        if (activeEffects == 0 && pendingCancellationSettlements == 0) {
                            terminal = CompletableDeferred()
                        }
                        pendingCancellationSettlements += pending.size
                    }
                }
            }
            certainty = if (effectMayHaveOccurred || inFlightDispatchAttempts > 0) {
                PhoneControlEffectCertainty.MAY_HAVE_OCCURRED
            } else {
                PhoneControlEffectCertainty.PROVEN_NO_EFFECT
            }
        }
        handlers.forEach { handler ->
            try {
                cancellationScope.launch {
                    try {
                        handler()
                    } catch (_: Throwable) {
                        // Failure is itself a terminal settlement of this cancellation attempt.
                    } finally {
                        finishCancellationSettlement()
                    }
                }
            } catch (_: Throwable) {
                finishCancellationSettlement()
            }
        }
        return certainty
    }

    /**
     * Registers cancellation before dispatch. A null result means cancellation already won and
     * the caller must not touch the platform.
     */
    fun registerCancellationHandler(handler: suspend () -> Unit): CancellationRegistration? =
        synchronized(lock) {
            if (cancellationRequested) return@synchronized null
            nextHandlerId += 1
            val id = nextHandlerId
            cancellationHandlers[id] = handler
            CancellationRegistration { synchronized(lock) { cancellationHandlers.remove(id) } }
        }

    /**
     * Atomically crosses the cancellation boundary into a structurally in-flight effect.
     * The returned lease must be closed only after the platform effect is terminal.
     */
    fun beginEffect(): EffectLease? = synchronized(lock) {
        effectBoundaryObserved = true
        if (cancellationRequested) return@synchronized null
        if (activeEffects == 0 && pendingCancellationSettlements == 0) {
            terminal = CompletableDeferred()
        }
        activeEffects += 1
        EffectLease(this)
    }

    suspend fun awaitTerminalEffects() {
        val signal = synchronized(lock) {
            if (activeEffects == 0 && pendingCancellationSettlements == 0) null else terminal
        }
        signal?.await()
    }

    fun terminalCertainty(mutatingFallback: Boolean): PhoneControlEffectCertainty =
        synchronized(lock) {
            when {
                effectMayHaveOccurred || inFlightDispatchAttempts > 0 ->
                    PhoneControlEffectCertainty.MAY_HAVE_OCCURRED
                effectBoundaryObserved -> PhoneControlEffectCertainty.PROVEN_NO_EFFECT
                mutatingFallback -> PhoneControlEffectCertainty.UNKNOWN
                else -> PhoneControlEffectCertainty.PROVEN_NO_EFFECT
            }
        }

    private fun markAccepted() {
        synchronized(lock) { effectMayHaveOccurred = true }
    }

    private fun tryReserveAcceptedDispatch(): Boolean = synchronized(lock) {
        if (cancellationRequested) return@synchronized false
        effectMayHaveOccurred = true
        true
    }

    private fun beginDispatchAttempt(): Boolean = synchronized(lock) {
        if (cancellationRequested) return@synchronized false
        inFlightDispatchAttempts += 1
        true
    }

    private fun finishDispatchAttempt(effectMayHaveOccurred: Boolean) {
        synchronized(lock) {
            check(inFlightDispatchAttempts > 0) { "dispatch attempt already settled" }
            inFlightDispatchAttempts -= 1
            if (effectMayHaveOccurred) this.effectMayHaveOccurred = true
        }
    }

    private fun finishCancellationSettlement() {
        val completedSignal = synchronized(lock) {
            check(pendingCancellationSettlements > 0) { "cancellation settlement already finished" }
            pendingCancellationSettlements -= 1
            terminal.takeIf {
                activeEffects == 0 && pendingCancellationSettlements == 0
            }
        }
        completedSignal?.complete(Unit)
    }

    private fun dispatchBooleanIfActive(dispatch: () -> Boolean): Boolean? {
        if (!beginDispatchAttempt()) return null
        try {
            return dispatch().also { accepted -> finishDispatchAttempt(accepted) }
        } catch (error: Throwable) {
            finishDispatchAttempt(effectMayHaveOccurred = true)
            throw error
        }
    }

    private fun dispatchIfActive(dispatch: () -> Unit): Boolean {
        if (!beginDispatchAttempt()) return false
        try {
            dispatch()
            finishDispatchAttempt(effectMayHaveOccurred = true)
            return true
        } catch (error: Throwable) {
            finishDispatchAttempt(effectMayHaveOccurred = true)
            throw error
        }
    }

    private fun finishEffect() {
        val completedSignal = synchronized(lock) {
            check(activeEffects > 0) { "effect lease already closed" }
            activeEffects -= 1
            terminal.takeIf {
                activeEffects == 0 && pendingCancellationSettlements == 0
            }
        }
        completedSignal?.complete(Unit)
    }

    internal class EffectLease internal constructor(
        private val owner: PhoneControlEffectOwner,
    ) : AutoCloseable {
        private val lock = Any()
        private var closed = false

        fun markAccepted() {
            synchronized(lock) {
                check(!closed) { "cannot accept a terminal effect" }
                owner.markAccepted()
            }
        }

        /** Reserves an effectful dispatch which the caller must perform immediately without suspension. */
        fun tryReserveAcceptedDispatch(): Boolean = synchronized(lock) {
            check(!closed) { "cannot dispatch a terminal effect" }
            owner.tryReserveAcceptedDispatch()
        }

        /** Holds the cancellation boundary across a short synchronous Android platform dispatch. */
        fun dispatchBooleanIfActive(dispatch: () -> Boolean): Boolean? = synchronized(lock) {
            check(!closed) { "cannot dispatch a terminal effect" }
            owner.dispatchBooleanIfActive(dispatch)
        }

        /** Dispatches a short synchronous void platform call if cancellation has not won. */
        fun dispatchIfActive(dispatch: () -> Unit): Boolean = synchronized(lock) {
            check(!closed) { "cannot dispatch a terminal effect" }
            owner.dispatchIfActive(dispatch)
        }

        override fun close() {
            val shouldClose = synchronized(lock) {
                if (closed) false else true.also { closed = true }
            }
            if (shouldClose) owner.finishEffect()
        }
    }

    internal fun interface CancellationRegistration : AutoCloseable {
        override fun close()
    }
}

internal suspend fun currentPhoneControlEffectOwner(): PhoneControlEffectOwner? =
    currentCoroutineContext()[PhoneControlEffectOwner]
