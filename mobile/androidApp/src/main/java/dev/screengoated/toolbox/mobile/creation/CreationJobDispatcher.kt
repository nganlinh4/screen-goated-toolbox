package dev.screengoated.toolbox.mobile.creation

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

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
                while (true) {
                    val pending = pendingSnapshot()
                    preparationRecovery.retain(pending.mapTo(mutableSetOf()) { it.jobId })
                    if (pending.isEmpty()) break
                    if (!dispatchRound(pending)) delay(DISPATCH_RETRY_DELAY_MS)
                }
            }
        }
    }

    fun signal() {
        signal.trySend(Unit)
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
        const val DISPATCH_RETRY_DELAY_MS = 1_000L
    }
}
