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
    val publicMessage = if (request?.tool == CreationTool.IMAGE_CREATOR.wireName) {
        publicImageCreationFailure()
    } else {
        publicCreationFailure()
    }
    memory.jobs[jobId] = current.copy(
        stage = "failed",
        progressText = "Could not create result.",
        phase = "failed",
        error = publicMessage,
    )
    return CreationFailureTransition(
        CreationFailureRecord(
            request?.tool,
            publicCreationFailureCategory(failureCode),
        ),
        request?.outputPath,
        request?.imagePaths.orEmpty(),
    )
}
