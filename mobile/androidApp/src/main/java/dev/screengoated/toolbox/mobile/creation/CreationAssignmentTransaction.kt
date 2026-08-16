package dev.screengoated.toolbox.mobile.creation

internal data class CreationWorkerAssignmentChange(
    val rollback: () -> Unit,
    val retiredInputPaths: List<String>,
)

internal fun applyCreationWorkerAssignment(
    memory: CreationManagerMemory,
    request: CreationWorkerRequest,
    assignedEngine: String,
): CreationWorkerAssignmentChange {
    val previousEngine = memory.engineIds[request.jobId]
    val invalidatedContinuations = if (request.operation == "generate") {
        memory.continuations.filterValues { it.engineId == assignedEngine }
    } else {
        emptyMap()
    }
    val invalidatedStatuses = invalidatedContinuations.keys.associateWith(memory.jobs::get)
    memory.engineIds[request.jobId] = assignedEngine
    invalidatedContinuations.keys.forEach { id ->
        memory.continuations.remove(id)
        memory.jobs[id]?.let { status ->
            memory.jobs[id] = status.copy(canSegment = false)
        }
    }
    return CreationWorkerAssignmentChange(
        rollback = {
        if (previousEngine == null) {
            memory.engineIds.remove(request.jobId)
        } else {
            memory.engineIds[request.jobId] = previousEngine
        }
        invalidatedContinuations.forEach { (id, continuation) ->
            memory.continuations[id] = continuation
        }
        invalidatedStatuses.forEach { (id, status) ->
            if (status == null) memory.jobs.remove(id) else memory.jobs[id] = status
        }
        },
        retiredInputPaths = invalidatedContinuations.values
            .map(CreationContinuation::sourcePath),
    )
}

internal class CreationWorkerAssignmentCoordinator(
    private val memory: CreationManagerMemory,
    private val mutationLock: Any,
    private val lock: Any,
    private val journalWriter: CreationManagerJournalWriter,
    private val recoveryLeases: CreationRecoveryWorkerLeases,
    private val files: CreationFileStore,
) {
    fun record(request: CreationWorkerRequest, assignedEngine: String) {
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
        CreationTool.fromWireName(request.tool)?.let { tool ->
            recoveryLeases.assign(request.jobId, tool, assignedEngine)
        }
        files.releaseJobInputs(retired)
    }
}
