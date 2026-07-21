package dev.screengoated.toolbox.mobile.creation

internal enum class CreationNativeTab { JOBS, RESULTS }

internal enum class CreationNativeStage {
    DRAFT,
    QUEUED,
    RUNNING,
    DONE,
    FAILED,
    CANCELLED,
}

internal data class CreationNativeItem(
    val id: String,
    val batchId: String,
    val sourcePath: String,
    val sourceName: String,
    val polycount: Int = CreationContract.DEFAULT_POLYCOUNT,
    val autoSegment: Boolean = false,
    val model: String = "simple",
    val submitted: Boolean = false,
    val stage: CreationNativeStage = CreationNativeStage.DRAFT,
    val status: CreationJobStatus? = null,
)

internal data class CreationNativeUiState(
    val tab: CreationNativeTab = CreationNativeTab.JOBS,
    val items: List<CreationNativeItem> = emptyList(),
    val selectedItemId: String? = null,
    val history: List<CreationHistoryEntry> = emptyList(),
    val selectedHistoryId: String? = null,
    val outputDirectory: String = "",
    val preparationStatus: String = "preparing",
    val transientError: String? = null,
) {
    val selectedItem: CreationNativeItem?
        get() = items.firstOrNull { it.id == selectedItemId }

    val selectedHistory: CreationHistoryEntry?
        get() = history.firstOrNull { it.id == selectedHistoryId }

    val runningCount: Int
        get() = items.count { it.stage == CreationNativeStage.RUNNING }
}

internal fun CreationJobStatus.toNativeStage(): CreationNativeStage = when (stage) {
    "done" -> CreationNativeStage.DONE
    "failed" -> CreationNativeStage.FAILED
    "cancelled" -> CreationNativeStage.CANCELLED
    "preparing", "visualizing", "generating", "segmenting", "finalizing" ->
        CreationNativeStage.RUNNING
    else -> CreationNativeStage.QUEUED
}

internal fun CreationNativeStage.isTerminal(): Boolean = this in setOf(
    CreationNativeStage.DONE,
    CreationNativeStage.FAILED,
    CreationNativeStage.CANCELLED,
)
