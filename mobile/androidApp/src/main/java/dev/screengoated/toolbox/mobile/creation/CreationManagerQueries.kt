package dev.screengoated.toolbox.mobile.creation

internal fun creationManagerStatus(
    memory: CreationManagerMemory,
    ownerId: String,
    tool: CreationTool,
    jobId: String?,
    nowMs: Long,
): CreationJobStatus {
    val current = jobId?.takeIf { memory.owners[it] == ownerId }?.let(memory.jobs::get)
        ?: memory.jobs.values.lastOrNull { memory.matchesOwnerTool(it, ownerId, tool) }
        ?: CreationJobFactory.idleStatus(tool)
    return current.withCreationElapsed(current.jobId?.let(memory.startedAt::get), nowMs)
}

internal fun creationManagerStatuses(
    memory: CreationManagerMemory,
    ownerId: String,
    tool: CreationTool,
    nowMs: Long,
): List<CreationJobStatus> = memory.jobs.values
    .filter { memory.matchesOwnerTool(it, ownerId, tool) }
    .map { it.withCreationElapsed(it.jobId?.let(memory.startedAt::get), nowMs) }

private fun CreationManagerMemory.matchesOwnerTool(
    status: CreationJobStatus,
    ownerId: String,
    tool: CreationTool,
): Boolean = status.jobId?.let(requests::get)?.tool?.let(CreationTool::fromWireName) == tool &&
    owners[status.jobId] == ownerId
