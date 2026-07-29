package dev.screengoated.toolbox.mobile.creation

internal data class CreationFailureRecord(
    val tool: String?,
    val category: String,
)

internal data class CreationWorkerEnvelope(
    val engineId: String,
    val event: CreationWorkerEvent,
)

internal data class CreationCompletionSnapshot(
    val request: CreationWorkerRequest,
    val status: CreationJobStatus,
    val ownerId: String,
    val destination: String?,
)

internal data class RestoredCreationRecord(
    val ownerId: String,
    val request: CreationWorkerRequest,
    val status: CreationJobStatus,
    val startedAtMs: Long,
    val destination: String?,
    val engineId: String?,
    val continuation: CreationContinuation?,
)

internal fun loadCreationManagerState(
    journal: CreationJobJournal,
    files: CreationFileStore,
    nowMs: Long,
    continuationLifetimeMs: Long,
): List<RestoredCreationRecord> {
    val loaded = journal.load()
    val records = boundedRestorableCreationRecords(
        loaded,
        CreationContract.MAXIMUM_PARALLEL_JOBS + 50,
    ) { record ->
        restoredCreationRecordIsBounded(record, nowMs) &&
            files.restoredRequestIsValid(record.request)
    }
    return records.mapNotNull { original ->
        val record = if (
            creationStageIsBusy(original.status.stage) &&
            nowMs >= original.request.deadlineAtMs
        ) {
            original.copy(
                status = original.status.copy(
                    stage = "failed",
                    progressText = "Creation timed out.",
                    phase = "failed",
                    error = CreationTool.fromWireName(original.request.tool)
                        ?.let(::publicCreationFailure),
                ),
            )
        } else {
            original
        }
        val busy = creationStageIsBusy(record.status.stage)
        if (!busy && record.status.outputPath?.let(files::exists) != true) {
            return@mapNotNull null
        }
        RestoredCreationRecord(
            ownerId = record.ownerId,
            request = record.request,
            status = record.status.copy(dispatchId = record.request.dispatchId),
            startedAtMs = record.startedAtMs,
            destination = record.destination,
            engineId = record.engineId,
            continuation = record.continuation
                ?.takeIf {
                    nowMs - it.createdAtMs <= continuationLifetimeMs &&
                        it.sourcePath in record.request.imagePaths &&
                        files.exists(it.sourcePath)
                }
                ?.let { saved ->
                    CreationContinuation(
                        ownerId = record.ownerId,
                        engineId = saved.engineId,
                        token = saved.token,
                        sourcePath = saved.sourcePath,
                        outputPath = saved.outputPath,
                        outputName = saved.outputName,
                        createdAtMs = saved.createdAtMs,
                    )
                },
        )
    }
}

internal fun CreationJobStatus.creationSourceHandles(): List<String> =
    sourceImagePaths.ifEmpty {
        listOfNotNull(sourceImagePath?.takeIf(String::isNotBlank))
    }

internal fun snapshotCreationManagerState(
    memory: CreationManagerMemory,
): List<CreationJournalRecord> = snapshotCreationManagerState(
    memory.jobs,
    memory.requests,
    memory.startedAt,
    memory.continuations,
    memory.engineIds,
    memory.owners,
    memory.destinations,
)

@Suppress("LongParameterList")
private fun snapshotCreationManagerState(
    jobs: Map<String, CreationJobStatus>,
    requests: Map<String, CreationWorkerRequest>,
    startedAt: Map<String, Long>,
    continuations: Map<String, CreationContinuation>,
    engineIds: Map<String, String>,
    owners: Map<String, String>,
    destinations: Map<String, String?>,
): List<CreationJournalRecord> = jobs.mapNotNull { (jobId, status) ->
        val request = requests[jobId] ?: return@mapNotNull null
        val ownerId = owners[jobId] ?: return@mapNotNull null
        CreationJournalRecord(
            ownerId = ownerId,
            request = request,
            status = status,
            startedAtMs = startedAt[jobId] ?: System.currentTimeMillis(),
            destination = destinations[jobId],
            engineId = engineIds[jobId],
            continuation = continuations[jobId]?.let { continuation ->
                CreationJournalContinuation(
                    engineId = continuation.engineId,
                    token = continuation.token,
                    sourcePath = continuation.sourcePath,
                    outputPath = continuation.outputPath,
                    outputName = continuation.outputName,
                    createdAtMs = continuation.createdAtMs,
                )
            },
        )
    }

internal fun restoreCreationManagerMemory(
    records: List<RestoredCreationRecord>,
    memory: CreationManagerMemory,
    recoveryLeases: CreationRecoveryWorkerLeases,
) {
    records.forEach { record ->
        val jobId = record.request.jobId
        memory.jobs[jobId] = record.status
        memory.requests[jobId] = record.request
        memory.startedAt[jobId] = record.startedAtMs
        memory.owners[jobId] = record.ownerId
        memory.destinations[jobId] = record.destination
        record.engineId?.let { memory.engineIds[jobId] = it }
        record.continuation?.let { memory.continuations[jobId] = it }
        if (creationStageIsBusy(record.status.stage)) {
            CreationTool.fromWireName(record.request.tool)?.let { tool ->
                recoveryLeases.acquire(jobId, tool)
            }
        }
    }
}

internal fun restoreCreationManagerJournal(
    journal: CreationJobJournal,
    files: CreationFileStore,
    cancellations: CreationCancellationStore,
    memory: CreationManagerMemory,
    recoveryLeases: CreationRecoveryWorkerLeases,
    continuationLifetimeMs: Long,
    maximumTerminalJobs: Int,
) {
    restoreCreationManagerMemory(
        cancellations.applyTo(
            loadCreationManagerState(
                journal,
                files,
                System.currentTimeMillis(),
                continuationLifetimeMs,
            ),
        ),
        memory,
        recoveryLeases,
    )
    memory.requests.values.filter { memory.jobs[it.jobId]?.stage == "cancelled" }
        .forEach(files::retireCancelledCreationRequest)
    memory.requests.values.filter { request ->
        memory.jobs[request.jobId]?.let { it.stage == "failed" &&
            System.currentTimeMillis() >= request.deadlineAtMs
        } == true
    }.forEach(files::retireCancelledCreationRequest)
    val retirement = memory.pruneTerminal(
        System.currentTimeMillis(),
        continuationLifetimeMs,
        maximumTerminalJobs,
    )
    files.queueJobInputCleanup(retirement.retiredInputPaths)
    runCatching { journal.save(snapshotCreationManagerState(memory)) }.onFailure {
        retirement.rollback(memory)
    }.getOrThrow()
    files.drainJobInputCleanup()
    files.reconcileJobInputOwnership()
    files.reconcilePersistedUriGrants()
}
