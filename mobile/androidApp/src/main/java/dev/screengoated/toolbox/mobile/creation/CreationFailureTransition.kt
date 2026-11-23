package dev.screengoated.toolbox.mobile.creation

internal data class CreationFailureTransition(
    val record: CreationFailureRecord,
    val outputPath: String?,
    val inputPaths: List<String>,
)

internal fun applyCreationFailure(
    memory: CreationManagerMemory,
    jobId: String,
    failureCode: String?,
): CreationFailureTransition? {
    val current = memory.jobs[jobId] ?: return null
    if (!creationStageIsBusy(current.stage)) return null
    val request = memory.requests[jobId]
    memory.jobs[jobId] = creationFailedStatus(current)
    return CreationFailureTransition(
        CreationFailureRecord(
            request?.tool,
            publicCreationFailureCategory(failureCode),
        ),
        request?.outputPath,
        listOfNotNull(request?.imagePath),
    )
}

/** The one public terminal-failure projection of a job status. */
internal fun creationFailedStatus(current: CreationJobStatus): CreationJobStatus = current.copy(
    stage = "failed",
    progressText = "Could not create result.",
    phase = "failed",
    error = publicCreationFailure(),
)
