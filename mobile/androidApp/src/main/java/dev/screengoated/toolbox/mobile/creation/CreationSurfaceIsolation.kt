package dev.screengoated.toolbox.mobile.creation

internal fun creationCancellationJobIds(
    memory: CreationManagerMemory,
    ownerId: String,
    tool: CreationTool,
    requestedJobId: String?,
): List<String> = if (requestedJobId != null) {
    listOfNotNull(requestedJobId.takeIf { memory.owners[it] == ownerId })
} else {
    memory.jobs.values
        .filter {
            val jobId = it.jobId
            val requestTool = jobId?.let(memory.requests::get)?.tool
                ?.let { wireName -> CreationTool.fromWireName(wireName) }
            requestTool == tool &&
                memory.owners[jobId] == ownerId &&
                creationStageIsBusy(it.stage)
        }
        .mapNotNull(CreationJobStatus::jobId)
}
