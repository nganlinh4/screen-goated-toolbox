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
    val referencePaths: List<String> = sourcePath.takeIf(String::isNotBlank)?.let(::listOf).orEmpty(),
    val generationMode: String = CreationGenerationMode.QUALITY.wireName,
    val polycount: Int = CreationContract.DEFAULT_POLYCOUNT,
    val autoSegment: Boolean = false,
    val model: String = "simple",
    val prompt: String = "",
    val submitted: Boolean = false,
    val stage: CreationNativeStage = CreationNativeStage.DRAFT,
    val status: CreationJobStatus? = null,
    val depthPreviewPath: String? = null,
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
    "preparing", "authenticating", "verifying", "profiling", "uploading",
    "visualizing", "generating", "segmenting", "finalizing" ->
        CreationNativeStage.RUNNING
    else -> CreationNativeStage.QUEUED
}

internal fun CreationNativeStage.isTerminal(): Boolean = this in setOf(
    CreationNativeStage.DONE,
    CreationNativeStage.FAILED,
    CreationNativeStage.CANCELLED,
)

internal fun CreationNativeItem.isConfigurable(): Boolean =
    (!submitted && stage == CreationNativeStage.DRAFT) || stage.isTerminal()

internal fun CreationNativeUiState.submitSelectedItem(): CreationNativeUiState {
    val selected = selectedItem ?: return this
    if (selected.stage == CreationNativeStage.RUNNING ||
        selected.submitted && !selected.stage.isTerminal()
    ) {
        return this
    }
    return copy(
        items = items.map { item ->
            if (item.id == selected.id) {
                item.copy(
                    submitted = true,
                    stage = CreationNativeStage.QUEUED,
                    status = null,
                    depthPreviewPath = null,
                )
            } else {
                item
            }
        },
        transientError = null,
    )
}

internal fun CreationNativeUiState.cancelActiveItems(): CreationNativeUiState = copy(
    items = items.map { item ->
        if (item.stage == CreationNativeStage.QUEUED ||
            item.stage == CreationNativeStage.RUNNING
        ) item.copy(stage = CreationNativeStage.CANCELLED, submitted = true)
        else item
    },
)
