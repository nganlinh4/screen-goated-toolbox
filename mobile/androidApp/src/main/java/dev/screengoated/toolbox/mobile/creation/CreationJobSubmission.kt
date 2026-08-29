package dev.screengoated.toolbox.mobile.creation

internal fun applyCreationJobSubmission(
    memory: CreationManagerMemory,
    queue: CreationDispatchQueue,
    draft: CreationJobDraft,
    ownerId: String,
    destination: String?,
    startedAtMs: Long,
    maximumQueuedJobs: Int,
): (() -> Unit)? {
    val request = draft.request
    val tool = requireNotNull(CreationTool.fromWireName(request.tool))
    if (queue.count(tool) >= maximumQueuedJobs ||
        !queue.offer(CreationPendingDispatch(request.jobId, tool))
    ) return null
    val jobId = request.jobId
    memory.jobs[jobId] = draft.status
    memory.requests[jobId] = request
    memory.startedAt[jobId] = startedAtMs
    memory.owners[jobId] = ownerId
    memory.destinations[jobId] = destination
    return {
        queue.remove(jobId)
        memory.jobs.remove(jobId)
        memory.requests.remove(jobId)
        memory.startedAt.remove(jobId)
        memory.owners.remove(jobId)
        memory.destinations.remove(jobId)
    }
}
