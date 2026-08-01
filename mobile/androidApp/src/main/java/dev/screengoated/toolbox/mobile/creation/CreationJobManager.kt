package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.channels.Channel
import kotlinx.serialization.json.JsonObject
internal class CreationJobManager private constructor(context: Context) {
    val files = CreationFileStore(context)
    val history = CreationHistoryStore(context, files)
    private val finisher = CreationJobFinisher(files, history)
    private val cancellations = CreationCancellationStore(context)
    private val ownerCloses = CreationOwnerCloseStore(context)
    private val deliveries = CreationDeliveryStore(context, files, cancellations)
    private val diagnostics = CreationDiagnostics(context)
    private val workers = CreationWorkerPool.get(context)
    private val journal = CreationJobJournal(context)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val startup = CompletableDeferred<Unit>()
    private val journalWriter = CreationManagerJournalWriter(journal, scope)
    private val eventSignal = Channel<Unit>(Channel.CONFLATED)
    private val deliveryRetrySignal = Channel<Unit>(Channel.CONFLATED)
    private val eventBuffer = CreationWorkerEventBuffer()
    private val dispatchQueue = CreationDispatchQueue(MAXIMUM_PENDING_JOBS_PER_TOOL)
    private val eventLock = Any()
    private val mutationLock = Any()
    private val lock = Any()
    private val memory = CreationManagerMemory()
    private val jobs = memory.jobs
    private val requests = memory.requests
    private val startedAt = memory.startedAt
    private val continuations = memory.continuations
    private val engineIds = memory.engineIds
    private val owners = memory.owners
    private val destinations = memory.destinations
    private val recoveryLeases = CreationRecoveryWorkerLeases(workers)
    private val ownerCloseCoordinator = CreationOwnerCloseCoordinator(
        ownerCloses, cancellations, files, workers, journalWriter, memory,
        dispatchQueue, mutationLock, lock, recoveryLeases,
    )
    private val durableStateReadable = creationDurableStateIsReadable(context.filesDir)
    private val deliveryCoordinator by lazy {
        CreationDeliveryCoordinator(
            deliveries, finisher, journal, journalWriter, memory,
            dispatchQueue, mutationLock, lock,
        )
    }
    private val historyCoordinator by lazy {
        CreationManagerHistoryCoordinator(
            history,
            files,
            memory,
            mutationLock,
            lock,
            journalWriter,
        )
    }
    private val dispatcher = CreationJobDispatcher(
        scope = scope,
        workers = workers,
        pendingSnapshot = { synchronized(lock) { dispatchQueue.snapshot() } },
        requestFor = { jobId ->
            synchronized(lock) {
                requests[jobId]?.takeIf {
                    jobs[jobId]?.stage?.let(::creationStageIsBusy) == true
                }
            }
        },
        removePending = { jobId ->
            synchronized(mutationLock) {
                synchronized(lock) { dispatchQueue.remove(jobId) }
            }
        },
        onAssigned = ::recordAssignment,
        isCancelled = { jobId -> synchronized(lock) { jobs[jobId]?.stage == "cancelled" } },
        onEvent = ::handleWorkerEvent,
        onDispatched = { request ->
            diagnostics.event(
                "job_dispatched",
                request.tool,
                jobId = request.jobId,
                stage = "preparing",
            )
        },
        onPreparationFailed = { jobId -> fail(jobId, "runtime_unavailable") },
    )
    init {
        workers.setPreparationStateListener(dispatcher::signal)
        scope.launch {
            runCatching {
                if (durableStateReadable) {
                    restoreJournal()
                    ownerCloseCoordinator.reconcileAll()
                    historyCoordinator.reconcileAtStartup()
                    deliveryCoordinator.reconcileAtStartup().forEach(recoveryLeases::release)
                    files.prunePresentationArtifacts()
                    history.maintain()
                }
                val deliveryJobs = deliveries.pendingJobIds()
                synchronized(lock) {
                    requests.values.filter { request ->
                        request.jobId !in deliveryJobs &&
                            jobs[request.jobId]?.stage?.let(::creationStageIsBusy) == true
                    }.forEach { request ->
                        check(
                            dispatchQueue.offer(
                                CreationPendingDispatch(
                                    request.jobId,
                                    CreationTool.fromWireName(request.tool)
                                        ?: return@forEach,
                                    engineIds[request.jobId],
                                ),
                            ),
                        ) { "Restored creation queue exceeds the product limit" }
                    }
                }
                startup.complete(Unit)
                dispatcher.signal()
                if (deliveryJobs.isNotEmpty()) deliveryRetrySignal.trySend(Unit)
            }.onFailure(startup::completeExceptionally)
        }
        scope.launch {
            for (ignored in eventSignal) {
                while (true) {
                    val envelope = synchronized(eventLock) { eventBuffer.poll() } ?: break
                    runCatching { processWorkerEvent(envelope.engineId, envelope.event) }
                        .onFailure {
                            envelope.event.jobId?.let(::fail)
                        }
                }
            }
        }
        scope.launch {
            for (ignored in deliveryRetrySignal) {
                var backoffMs = 250L
                while (deliveries.pendingJobIds().isNotEmpty()) {
                    runCatching { deliveryCoordinator.reconcileInProcess() }
                        .getOrDefault(emptyMap())
                        .forEach(recoveryLeases::release)
                    if (deliveries.pendingJobIds().isEmpty()) break
                    delay(backoffMs)
                    backoffMs = (backoffMs * 2).coerceAtMost(30_000L)
                }
            }
        }
    }
    suspend fun awaitStartup() = startup.await()
    fun acquireSurface(tool: CreationTool, ownerId: String): String =
        "preparing".also { workers.acquire(tool, "surface:$ownerId") }
    fun releaseSurface(tool: CreationTool, ownerId: String) =
        workers.release(tool, "surface:$ownerId")
    fun closeOwner(tool: CreationTool, ownerId: String) =
        ownerCloseCoordinator.requestWithRetry(scope, ownerId, tool)
    fun startOneShotPreparation() = workers.startOneShotPreparation()
    fun preparationStatus(tool: CreationTool): String = workers.preparationStatus(tool)
    fun supportsOptionalInstruction(mode: String): Boolean =
        workers.supportsOptionalInstruction(mode)
    fun removeRuntime() = workers.removeRuntime()
    fun startJob(
        ownerId: String,
        tool: CreationTool,
        args: JsonObject,
    ): CreationSubmissionOutcome {
        if (!durableStateReadable) {
            return CreationSubmissionOutcome.Rejected(
                CreationSubmissionFailure.STORAGE_UNAVAILABLE,
            )
        }
        var reservedOutputPath: String? = null
        var reservedInputs = emptyList<String>()
        return runCatching {
        val jobId = nextJobId(tool)
        val dispatchId = nextDispatchId(tool)
        val destination = files.outputDestinationSnapshot()
        val requestedMode = CreationGenerationMode.fromWireName(
            args["generationMode"]?.toString()?.trim('"'),
        )
        val draft = CreationJobFactory.create(
            tool,
            args,
            files,
            ownerId,
            jobId,
            dispatchId,
            destination,
            optionalInstructionAllowed = tool == CreationTool.IMAGE_TO_3D &&
                supportsOptionalInstruction(requestedMode.wireName),
        )
        val request = draft.request
        reservedOutputPath = request.outputPath
        reservedInputs = request.imagePaths
        val status = draft.status
        val accepted = synchronized(mutationLock) {
            var rollback: (() -> Unit)? = null
            val journalSnapshot = synchronized(lock) {
                cancellations.reserveAcceptance(request.dispatchId)
                rollback = applyCreationJobSubmission(
                    memory,
                    dispatchQueue,
                    draft,
                    ownerId,
                    destination,
                    request.acceptedAtMs,
                    MAXIMUM_QUEUED_JOBS_PER_TOOL,
                )
                rollback?.let { journalWriter.snapshot(memory) }
            }
            if (journalSnapshot == null) {
                false
            } else {
                runCatching { journalWriter.writeRequired(journalSnapshot) }.onFailure {
                    synchronized(lock) { requireNotNull(rollback).invoke() }
                }.getOrThrow()
                pruneTerminalDurably()
                true
            }
        }
        if (!accepted) {
            files.deleteReservedStagingFile(tool, request.outputPath)
            files.releaseJobInputs(request.imagePaths)
            reservedOutputPath = null
            reservedInputs = emptyList()
            return@runCatching CreationSubmissionOutcome.Rejected(
                CreationSubmissionFailure.QUEUE_FULL,
            )
        }
        reservedOutputPath = null
        reservedInputs = emptyList()
        workers.restartPreparation(tool)
        recoveryLeases.acquire(jobId, tool)
        diagnostics.event(
            "job_queued",
            tool.wireName,
            jobId = jobId,
            stage = "preparing",
        )
        dispatcher.signal()
        CreationSubmissionOutcome.Accepted(status)
        }.getOrElse { failure ->
            reservedOutputPath?.let { files.deleteReservedStagingFile(tool, it) }
            files.releaseJobInputs(reservedInputs)
            CreationSubmissionOutcome.Rejected(
                creationSubmissionFailure(failure),
            )
        }
    }
    suspend fun startSegmentation(ownerId: String, continuationId: String): CreationJobStatus {
        awaitStartup()
        if (!durableStateReadable) throw CreationStorageUnavailableException()
        val snapshot = synchronized(lock) {
            val current = continuations[continuationId]
                ?: error("This model can no longer be separated into parts")
            require(current.ownerId == ownerId) { "This result belongs to another session" }
            require(
                creationContinuationIsLive(
                    current.createdAtMs,
                    System.currentTimeMillis(),
                    CONTINUATION_LIFETIME_MS,
                )
            ) {
                "This model can no longer be separated into parts"
            }
            CreationSegmentationSnapshot(continuationId, current)
        }
        val destination = files.outputDestinationSnapshot()
        val draft = CreationJobFactory.createSegmentation(
            snapshot.continuation,
            files,
            ownerId,
            nextJobId(CreationTool.IMAGE_TO_3D),
            nextDispatchId(CreationTool.IMAGE_TO_3D),
            destination,
        )
        var retiredContinuationInputs = emptyList<String>()
        try {
            synchronized(mutationLock) {
                lateinit var rollback: () -> Unit
                val journalSnapshot = synchronized(lock) {
                    cancellations.reserveAcceptance(draft.request.dispatchId)
                    retiredContinuationInputs = continuations.values
                        .filter { it.engineId == snapshot.continuation.engineId }
                        .map(CreationContinuation::sourcePath)
                    rollback = applyCreationSegmentationSubmission(
                        memory = memory,
                        dispatchQueue = dispatchQueue,
                        snapshot = snapshot,
                        draft = draft,
                        ownerId = ownerId,
                        destination = destination,
                        startedAtMs = draft.request.acceptedAtMs,
                        maximumQueuedJobs = MAXIMUM_QUEUED_JOBS_PER_TOOL,
                    )
                    journalWriter.snapshot(memory)
                }
                runCatching { journalWriter.writeRequired(journalSnapshot) }.onFailure {
                    synchronized(lock) { rollback() }
                }.getOrThrow()
                pruneTerminalDurably()
            }
        } catch (failure: Throwable) {
            files.deleteReservedStagingFile(CreationTool.IMAGE_TO_3D, draft.request.outputPath)
            files.releaseJobInputs(draft.request.imagePaths)
            throw failure
        }
        files.releaseJobInputs(retiredContinuationInputs)
        recoveryLeases.acquire(draft.request.jobId, CreationTool.IMAGE_TO_3D)
        diagnostics.event(
            "job_queued",
            CreationTool.IMAGE_TO_3D.wireName,
            jobId = draft.request.jobId,
            stage = "segmenting",
        )
        dispatcher.signal()
        return draft.status
    }
    fun status(ownerId: String, tool: CreationTool, jobId: String?): CreationJobStatus =
        synchronized(lock) {
        val current = jobId?.takeIf { owners[it] == ownerId }?.let(jobs::get)
            ?: jobs.values.lastOrNull {
                requestTool(it.jobId) == tool && owners[it.jobId] == ownerId
            }
            ?: CreationJobFactory.idleStatus(tool)
        current.withCreationElapsed(current.jobId?.let(startedAt::get), System.currentTimeMillis())
    }

    fun statuses(ownerId: String, tool: CreationTool): List<CreationJobStatus> = synchronized(lock) {
        jobs.values.filter {
            requestTool(it.jobId) == tool && owners[it.jobId] == ownerId
        }.map {
            it.withCreationElapsed(it.jobId?.let(startedAt::get), System.currentTimeMillis())
        }
    }

    fun cancel(ownerId: String, tool: CreationTool, jobId: String?): List<CreationJobStatus> {
        val (targets, cancelledRequests) = synchronized(mutationLock) {
            val selected = synchronized(lock) {
                val ids = creationCancellationJobIds(memory, ownerId, tool, jobId)
                ids.mapNotNull { id ->
                    requests[id]?.takeIf { jobs[id]?.stage?.let(::creationStageIsBusy) == true }
                }
            }
            cancellations.record(selected)
            val snapshot = synchronized(lock) {
                val transitioned = mutableListOf<String>()
                selected.forEach { request ->
                    val id = request.jobId
                    jobs[id]?.takeIf { creationStageIsBusy(it.stage) }?.let {
                        jobs[id] = it.copy(stage = "cancelled", progressText = "Cancelled.")
                        transitioned += id
                    }
                    dispatchQueue.remove(id)
                }
                transitioned to journalWriter.snapshot(memory)
            }
            runCatching { journalWriter.writeRequired(snapshot.second) }.onFailure {
                synchronized(lock) { schedulePersistLocked() }
            }.onSuccess {
                pruneTerminalDurably()
            }
            snapshot.first to selected
        }
        targets.forEach(workers::cancel)
        cancelledRequests.forEach(files::retireCancelledCreationRequest)
        targets.forEach { recoveryLeases.release(it, tool) }
        dispatcher.signal()
        if (deliveries.pendingJobIds().isNotEmpty()) deliveryRetrySignal.trySend(Unit)
        return statuses(ownerId, tool)
    }

    fun renameHistory(tool: CreationTool, id: String, name: String): CreationHistoryEntry =
        historyCoordinator.rename(tool, id, name)

    fun deleteHistory(tool: CreationTool, id: String) =
        historyCoordinator.delete(tool, id)

    fun deleteAllHistory(tool: CreationTool) =
        historyCoordinator.deleteAll(tool)
    private fun recordAssignment(request: CreationWorkerRequest, assignedEngine: String) {
        val retired = synchronized(mutationLock) {
            lateinit var change: CreationWorkerAssignmentChange
            val snapshot = synchronized(lock) {
                change = applyCreationWorkerAssignment(memory, request, assignedEngine)
                journalWriter.snapshot(memory)
            }
            runCatching { journalWriter.writeRequired(snapshot) }.onFailure {
                synchronized(lock) { change.rollback() }
            }.getOrThrow()
            change.retiredInputPaths
        }
        files.releaseJobInputs(retired)
    }

    private fun handleWorkerEvent(engineId: String, event: CreationWorkerEvent) {
        if (event.jobId == null) return
        synchronized(eventLock) {
            eventBuffer.offer(CreationWorkerEnvelope(engineId, event))
        }
        eventSignal.trySend(Unit)
    }
    private fun processWorkerEvent(engineId: String, event: CreationWorkerEvent) {
        val jobId = event.jobId ?: return
        val expectedRequest = synchronized(lock) { requests[jobId] } ?: return
        val expectedEngine = synchronized(lock) { engineIds[jobId] }
        if (expectedEngine != null && expectedEngine != engineId) return
        if (expectedRequest.generationMode != null &&
            event.generationMode != null &&
            event.generationMode != expectedRequest.generationMode
        ) {
            fail(jobId)
            return
        }
        when (event.event) {
            "success" -> finish(engineId, jobId, event)
            "failure" -> fail(jobId, event.failureCode)
            "execution_lost" -> {
                synchronized(mutationLock) {
                    synchronized(lock) {
                        dispatchQueue.offer(
                            CreationPendingDispatch(
                                jobId,
                                CreationTool.fromWireName(expectedRequest.tool) ?: return,
                                expectedEngine,
                            ),
                        )
                    }
                }
                dispatcher.signal()
            }
            "cancelled" -> {
                val ownerId = synchronized(lock) { owners[jobId] } ?: return
                cancel(ownerId, requestTool(jobId) ?: return, jobId)
            }
            else -> updateProgress(jobId, event)
        }
    }

    private fun updateProgress(jobId: String, event: CreationWorkerEvent) {
        val update = synchronized(mutationLock) {
            synchronized(lock) {
                applyCreationProgressUpdate(memory, jobId, event)?.also {
                    if (it.stageChanged) schedulePersistLocked()
                }
            }
        }
        update?.diagnosticStage?.let { stage ->
            diagnostics.event(
                "job_progress",
                update.tool,
                jobId = jobId,
                stage = stage,
            )
        }
    }

    private fun finish(engineId: String, jobId: String, event: CreationWorkerEvent) {
        val snapshot = synchronized(lock) {
            val request = requests[jobId] ?: return
            val current = jobs[jobId] ?: return
            if (!creationStageIsBusy(current.stage)) return
            CreationCompletionSnapshot(
                request,
                current,
                owners[jobId] ?: return,
                destinations[jobId],
            )
        }
        val prepared = runCatching {
            finisher.prepare(
                engineId,
                snapshot.ownerId,
                snapshot.request,
                snapshot.status,
                event,
            )
        }.getOrElse {
            fail(jobId)
            return
        }
        val stillLive = synchronized(lock) {
            requests[jobId] == snapshot.request &&
                jobs[jobId]?.stage?.let(::creationStageIsBusy) == true
        }
        if (!stillLive) {
            files.deleteManagedPath(prepared.stagingPath)
            return
        }
        val completed = runCatching {
            synchronized(mutationLock) { cancellations.ifActive(snapshot.request) {
                val completed = deliveryCoordinator.deliver(prepared, snapshot.destination)
                finisher.recordHistory(
                    completed,
                    event,
                    protectedPaths = synchronized(lock) { memory.liveArtifactPaths() },
                )
                deliveries.markHistoryCommitted(completed.request.dispatchId)
                var previous: CreationJobStatus? = null
                val durable = synchronized(lock) {
                    val current = jobs[jobId]
                    require(
                        requests[jobId] == snapshot.request &&
                            current?.stage?.let(::creationStageIsBusy) == true,
                    ) { "Creation job changed before terminal commit" }
                    previous = current
                    jobs[jobId] = completed.status
                    completed.continuation?.let { continuations[jobId] = it }
                    journalWriter.snapshot(memory)
                }
                runCatching { journalWriter.writeRequired(durable) }.onFailure {
                    synchronized(lock) {
                        jobs[jobId] = requireNotNull(previous)
                        continuations.remove(jobId)
                    }
                }.getOrThrow()
                pruneTerminalDurably()
                if (!deliveries.complete(completed.request.dispatchId)) {
                    deliveryRetrySignal.trySend(Unit)
                }
                files.releaseJobInputs(
                    creationJobInputPathsReleasedAfterCommit(
                        completed.request,
                        retainedByContinuation = completed.continuation != null,
                    ),
                )
                completed
            } }
        }.getOrElse {
            val action = runCatching {
                creationDeliveryFailureAction(
                    deliveries.containsDispatch(snapshot.request.dispatchId),
                )
            }.getOrDefault(CreationDeliveryFailureAction.RETRY)
            if (action == CreationDeliveryFailureAction.RETRY) {
                deliveryRetrySignal.trySend(Unit)
            } else {
                fail(jobId)
            }
            return
        }
        recoveryLeases.release(jobId, CreationTool.fromWireName(completed.request.tool))
        diagnostics.event(
            "job_succeeded",
            completed.request.tool,
            jobId = jobId,
            stage = "done",
        )
        dispatcher.signal()
    }
    private fun fail(jobId: String, failureCode: String? = null) {
        val transition = synchronized(mutationLock) {
            val applied = synchronized(lock) {
                applyCreationFailure(memory, jobId, failureCode) ?: return
            }
            pruneTerminalDurably()
            applied
        }
        val tool = CreationTool.fromWireName(transition.record.tool)
        if (tool != null) {
            transition.outputPath?.let { files.deleteReservedStagingFile(tool, it) }
        }
        files.releaseJobInputs(transition.inputPaths)
        recoveryLeases.release(jobId, tool)
        diagnostics.event(
            "job_failed",
            transition.record.tool,
            jobId = jobId,
            stage = "failed",
            failureCategory = transition.record.category,
        )
        dispatcher.signal()
    }

    private fun restoreJournal() {
        restoreCreationManagerJournal(
            journal, files, cancellations, memory, recoveryLeases,
            CONTINUATION_LIFETIME_MS, MAXIMUM_TERMINAL_JOBS,
        )
    }

    private fun schedulePersistLocked() = journalWriter.schedule(journalWriter.snapshot(memory))

    private fun requestTool(jobId: String?): CreationTool? = jobId?.let(requests::get)
        ?.tool?.let(CreationTool::fromWireName)
    private fun pruneTerminalDurably() = pruneCreationManagerTerminalDurably(
        memory, journalWriter, files, lock, System.currentTimeMillis(),
        CONTINUATION_LIFETIME_MS, MAXIMUM_TERMINAL_JOBS,
    )

    companion object {
        @Volatile private var instance: CreationJobManager? = null

        fun get(context: Context): CreationJobManager = instance ?: synchronized(this) {
            instance ?: CreationJobManager(context.applicationContext).also { instance = it }
        }
    }
}
