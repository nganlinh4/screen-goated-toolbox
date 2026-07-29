package dev.screengoated.toolbox.mobile.creation

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

internal class CreationOwnerCloseCoordinator(
    private val store: CreationOwnerCloseStore,
    private val cancellations: CreationCancellationStore,
    private val files: CreationFileStore,
    private val workers: CreationWorkerPool,
    private val journalWriter: CreationManagerJournalWriter,
    private val memory: CreationManagerMemory,
    private val dispatchQueue: CreationDispatchQueue,
    private val mutationLock: Any,
    private val stateLock: Any,
    private val recoveryLeases: CreationRecoveryWorkerLeases,
) {
    fun request(ownerId: String, tool: CreationTool) {
        store.begin(ownerId, tool)
        val record = requireNotNull(store.pending().firstOrNull {
            it.ownerId == ownerId && it.tool == tool.wireName
        })
        reconcile(record)
    }

    fun requestWithRetry(scope: CoroutineScope, ownerId: String, tool: CreationTool) {
        runCatching { request(ownerId, tool) }.onFailure {
            scope.launch {
                var backoffMs = 250L
                while (runCatching { request(ownerId, tool) }.isFailure) {
                    delay(backoffMs)
                    backoffMs = (backoffMs * 2).coerceAtMost(30_000L)
                }
            }
        }
    }

    fun reconcileAll() {
        store.pending().forEach { record ->
            reconcile(record)
        }
    }

    private fun reconcile(saved: CreationOwnerCloseRecord) {
        val tool = requireNotNull(CreationTool.fromWireName(saved.tool))
        var record = saved
        if (!record.stateRetired) {
            val requests = synchronized(stateLock) {
                memory.requests.values.filter { request ->
                    memory.owners[request.jobId] == record.ownerId &&
                        request.tool == tool.wireName
                }
            }
            val busyIds = synchronized(stateLock) {
                requests.mapNotNullTo(mutableSetOf()) { request ->
                    request.jobId.takeIf {
                        memory.jobs[it]?.stage?.let(::creationStageIsBusy) == true
                    }
                }
            }
            record = store.prepare(record.ownerId, tool, requests, busyIds)
            cancellations.record(record.requests.filter { it.jobId in record.busyJobIds })
            lateinit var retirement: CreationMemoryRetirement
            synchronized(mutationLock) {
                val snapshot = synchronized(stateLock) {
                    retirement = memory.retireOwner(record.ownerId, tool)
                    journalWriter.snapshot(memory)
                }
                runCatching { journalWriter.writeRequired(snapshot) }.onFailure {
                    synchronized(stateLock) { retirement.rollback(memory) }
                }.getOrThrow()
                synchronized(stateLock) {
                    retirement.original.forEach { dispatchQueue.remove(it.id) }
                }
            }
            record = store.markStateRetired(record.ownerId, tool)
        }
        record.requests.forEach { request ->
            if (request.jobId in record.busyJobIds) {
                workers.cancel(request.jobId)
                check(files.retireCancelledCreationRequest(request)) {
                    CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
                }
                recoveryLeases.release(request.jobId, tool)
            } else {
                check(files.releaseJobInputs(request.imagePaths)) {
                    CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
                }
            }
        }
        store.complete(record.ownerId, tool)
    }
}
