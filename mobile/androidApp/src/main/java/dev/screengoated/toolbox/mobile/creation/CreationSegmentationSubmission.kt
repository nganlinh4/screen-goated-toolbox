package dev.screengoated.toolbox.mobile.creation

internal data class CreationSegmentationSnapshot(
    val continuationId: String,
    val continuation: CreationContinuation,
)

internal fun applyCreationSegmentationSubmission(
    memory: CreationManagerMemory,
    dispatchQueue: CreationDispatchQueue,
    snapshot: CreationSegmentationSnapshot,
    draft: CreationJobDraft,
    ownerId: String,
    destination: String?,
    startedAtMs: Long,
    maximumQueuedJobs: Int,
): () -> Unit {
    val current = memory.continuations[snapshot.continuationId]
    require(current == snapshot.continuation) {
        "This model can no longer be separated into parts"
    }
    val tool = CreationTool.IMAGE_TO_3D
    require(dispatchQueue.count(tool) < maximumQueuedJobs) {
        "Creation queue is full"
    }
    val request = draft.request
    val jobId = request.jobId
    val affectedContinuations = memory.continuations.filterValues {
        it.engineId == current.engineId
    }
    val affectedStatuses = affectedContinuations.keys.associateWith(memory.jobs::get)
    check(dispatchQueue.offer(CreationPendingDispatch(jobId, tool, current.engineId))) {
        "Creation queue is full"
    }
    affectedContinuations.keys.forEach { continuationId ->
        memory.continuations.remove(continuationId)
        memory.jobs[continuationId]?.let { status ->
            memory.jobs[continuationId] = status.copy(canSegment = false)
        }
    }
    memory.jobs[jobId] = draft.status
    memory.requests[jobId] = request
    memory.startedAt[jobId] = startedAtMs
    memory.owners[jobId] = ownerId
    memory.destinations[jobId] = destination
    return {
        dispatchQueue.remove(jobId)
        memory.jobs.remove(jobId)
        memory.requests.remove(jobId)
        memory.startedAt.remove(jobId)
        memory.engineIds.remove(jobId)
        memory.owners.remove(jobId)
        memory.destinations.remove(jobId)
        affectedContinuations.forEach { (id, continuation) ->
            memory.continuations[id] = continuation
        }
        affectedStatuses.forEach { (id, status) ->
            if (status == null) memory.jobs.remove(id) else memory.jobs[id] = status
        }
    }
}
