package dev.screengoated.toolbox.mobile.creation

internal data class CreationNativeStatusRefresh(
    val state: CreationNativeUiState,
    val reachedTerminal: Boolean,
)

internal fun refreshCreationNativeItems(
    current: CreationNativeUiState,
    byJob: Map<String, CreationJobStatus>,
    verifiedHistory: List<CreationHistoryEntry>?,
    tool: CreationTool,
): CreationNativeStatusRefresh {
    var reachedTerminal = false
    val items = current.items.map { item ->
        val jobId = item.status?.jobId ?: return@map item
        val status = byJob[jobId] ?: if (
            verifiedHistory != null && item.stage == CreationNativeStage.RUNNING
        ) {
            reachedTerminal = true
            val recovered = verifiedHistory.firstOrNull {
                it.dispatchId != null && it.dispatchId == item.status.dispatchId
            }
            return@map if (recovered != null) {
                item.copy(
                    stage = CreationNativeStage.DONE,
                    status = item.status.copy(
                        stage = "done",
                        progressText = "Ready",
                        phase = "complete",
                        progressRatio = 1.0,
                        outputPath = recovered.outputPath,
                        outputName = recovered.outputName,
                        error = null,
                    ),
                )
            } else {
                item.copy(
                    stage = CreationNativeStage.FAILED,
                    status = item.status.copy(
                        stage = "failed",
                        progressText = "Could not create result.",
                        phase = "failed",
                        error = publicCreationFailure(tool),
                    ),
                )
            }
        } else {
            return@map item
        }
        val stage = status.toNativeStage()
        if (item.stage == CreationNativeStage.RUNNING &&
            stage != CreationNativeStage.RUNNING
        ) {
            reachedTerminal = true
        }
        item.copy(
            stage = stage,
            status = status,
            generationMode = status.generationMode ?: item.generationMode,
            polycount = status.polycount ?: item.polycount,
            autoSegment = status.autoSegment ?: item.autoSegment,
        )
    }
    return CreationNativeStatusRefresh(
        current.copy(items = items, history = verifiedHistory ?: current.history),
        reachedTerminal,
    )
}
