package dev.screengoated.toolbox.mobile.creation

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

internal class CreationJobDispatcher(
    scope: CoroutineScope,
    private val workers: CreationWorkerPool,
    private val pendingSnapshot: () -> List<CreationPendingDispatch>,
    private val requestFor: (String) -> CreationWorkerRequest?,
    private val removePending: (String) -> Boolean,
    private val onAssigned: (CreationWorkerRequest, String) -> Unit,
    private val isCancelled: (String) -> Boolean,
    private val onEvent: (String, CreationWorkerEvent) -> Unit,
    private val onDispatched: (CreationWorkerRequest) -> Unit,
    private val onPreparationFailed: (String) -> Unit,
) {
    private val signal = Channel<Unit>(Channel.CONFLATED)
    private val preparationRecovery = CreationPreparationRecoveryWindow(
        CreationContract.TEMPORARY_CAPACITY_RECOVERY_MS,
    )

    init {
        scope.launch {
            for (ignored in signal) {
                try {
                    drainPending()
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    Log.e(TAG, "Creation dispatch loop recovered after a failed round", error)
                    delay(DISPATCH_RETRY_DELAY_MS)
                    signal.trySend(Unit)
                }
            }
        }
    }

    fun signal() {
        signal.trySend(Unit)
    }

    private suspend fun drainPending() {
        while (true) {
            val pending = pendingSnapshot()
            preparationRecovery.retain(pending.mapTo(mutableSetOf()) { it.jobId })
            if (pending.isEmpty()) return
            if (!dispatchRound(pending)) {
                awaitCreationDispatchWake(
                    signal,
                    DISPATCH_RETRY_DELAY_MS,
                    CREATION_DISPATCH_IDLE_FALLBACK_MS,
                )
            }
        }
    }

    private fun dispatchRound(pendingSnapshot: List<CreationPendingDispatch>): Boolean {
        var dispatched = false
        pendingSnapshot.forEach { pending ->
            val request = requestFor(pending.jobId)
            if (request == null) {
                preparationRecovery.clear(pending.jobId)
                removePending(pending.jobId)
                dispatched = true
                return@forEach
            }
            val result = workers.dispatch(
                request,
                pending.preferredEngineId,
                onEvent,
            ) { assigned -> onAssigned(request, assigned) }
            when (result) {
                is CreationWorkerDispatchResult.Assigned -> {
                    preparationRecovery.clear(request.jobId)
                    removePending(request.jobId)
                    if (isCancelled(request.jobId)) workers.cancel(request.jobId)
                    onDispatched(request)
                    dispatched = true
                }
                CreationWorkerDispatchResult.TemporaryCapacityPause -> {
                    if (preparationRecovery.shouldRetry(
                            request.jobId,
                            request.deadlineAtMs,
                        )
                    ) {
                        workers.restartPreparation(pending.tool)
                    } else {
                        preparationRecovery.clear(request.jobId)
                        removePending(request.jobId)
                        onPreparationFailed(request.jobId)
                        dispatched = true
                    }
                }
                CreationWorkerDispatchResult.PreparationFailed -> {
                    preparationRecovery.clear(request.jobId)
                    removePending(request.jobId)
                    onPreparationFailed(request.jobId)
                    dispatched = true
                }
                CreationWorkerDispatchResult.Waiting -> Unit
            }
        }
        return dispatched
    }

    private companion object {
        const val TAG = "CreationDispatcher"
        const val DISPATCH_RETRY_DELAY_MS = 1_000L
    }
}

/** Longest idle wait after a round that dispatched nothing. */
internal const val CREATION_DISPATCH_IDLE_FALLBACK_MS = 15_000L

/**
 * Waits after a round that dispatched nothing. Worker release, cancel, enqueue and
 * preparation-state changes all signal, so the dispatcher sleeps on that signal
 * instead of polling the pool every second. The floor keeps signal bursts (such as
 * repeated preparation failures) to at most one round per [minimumMs].
 */
internal suspend fun awaitCreationDispatchWake(
    signal: ReceiveChannel<Unit>,
    minimumMs: Long,
    fallbackMs: Long,
) {
    delay(minimumMs)
    withTimeoutOrNull((fallbackMs - minimumMs).coerceAtLeast(0L)) { signal.receive() }
}
