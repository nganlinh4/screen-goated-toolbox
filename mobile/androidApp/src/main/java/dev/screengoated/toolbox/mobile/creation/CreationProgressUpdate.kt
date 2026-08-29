package dev.screengoated.toolbox.mobile.creation

internal data class CreationProgressUpdate(
    val tool: String?,
    val diagnosticStage: String?,
    val stageChanged: Boolean,
)

internal fun applyCreationProgressUpdate(
    memory: CreationManagerMemory,
    jobId: String,
    event: CreationWorkerEvent,
): CreationProgressUpdate? {
    val current = memory.jobs[jobId] ?: return null
    if (!creationStageIsBusy(current.stage)) return null
    val requestTool = memory.requests[jobId]?.tool
    val tool = CreationTool.fromWireName(requestTool) ?: return null
    val isImageCreator = tool == CreationTool.IMAGE_CREATOR
    val hasImageReferences = memory.requests[jobId]?.let { request ->
        request.imagePaths.any(String::isNotBlank) || request.imagePath.isNotBlank()
    } == true
    val observedStage = event.stage ?: current.stage
    val nextStage = publicCreationStage(
        tool,
        observedStage,
        current.stage,
        hasImageReferences,
    )
    val diagnosticStage = if (nextStage != current.stage) {
        if (isImageCreator) "image.$nextStage" else nextStage
    } else {
        null
    }
    memory.jobs[jobId] = current.copy(
        stage = nextStage,
        progressText = if (isImageCreator) {
            publicImageCreationText(nextStage, hasImageReferences)
        } else {
            publicCreationProgressText(nextStage)
        },
        phase = nextStage,
        progressRatio = event.progressRatio
            ?.takeIf { it.isFinite() && it in 0.0..1.0 }
            ?: current.progressRatio,
        estimatedTotalMs = event.estimatedTotalMs
            ?.takeIf { it in 1..CreationContract.MAXIMUM_JOB_RUNTIME_MS }
            ?: current.estimatedTotalMs,
        timingSampleCount = event.timingSampleCount
            ?.takeIf { it in 0L..MAXIMUM_PUBLIC_TIMING_SAMPLES }
            ?: current.timingSampleCount,
    )
    return CreationProgressUpdate(requestTool, diagnosticStage, nextStage != current.stage)
}

internal const val MAXIMUM_PUBLIC_TIMING_SAMPLES = 100_000L
