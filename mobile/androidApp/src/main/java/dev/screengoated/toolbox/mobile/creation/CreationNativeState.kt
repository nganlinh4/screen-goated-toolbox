package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.util.UUID

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
    val backgroundMode: String = "opaque",
    val prompt: String = "",
    val instruction: String = "",
    val allowsInstruction: Boolean = false,
    val submitted: Boolean = false,
    val stage: CreationNativeStage = CreationNativeStage.DRAFT,
    val status: CreationJobStatus? = null,
    val submissionToken: String? = null,
    val createdAtMs: Long = System.currentTimeMillis(),
)

internal data class CreationNativeUiState(
    val tab: CreationNativeTab = CreationNativeTab.JOBS,
    val items: List<CreationNativeItem> = emptyList(),
    val selectedItemId: String? = null,
    val history: List<CreationHistoryEntry> = emptyList(),
    val selectedHistoryId: String? = null,
    val outputDirectory: String = "",
    val preparationStatus: String = "ready",
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
    "preparing", "uploading",
    "generating", "segmenting", "refining", "finalizing" ->
        CreationNativeStage.RUNNING
    else -> CreationNativeStage.QUEUED
}

internal fun CreationNativeStage.isTerminal(): Boolean = this in setOf(
    CreationNativeStage.DONE,
    CreationNativeStage.FAILED,
    CreationNativeStage.CANCELLED,
)

internal fun creationSurfaceHasActiveWork(items: List<CreationNativeItem>): Boolean =
    items.any {
        it.submitted && (
            it.stage == CreationNativeStage.QUEUED ||
                it.stage == CreationNativeStage.RUNNING
            )
    }

internal fun creationDraftsForImport(
    paths: List<String>,
    batchId: String,
    idForIndex: (Int) -> String,
): List<CreationNativeItem> = paths.mapIndexed { index, path ->
    CreationNativeItem(
        id = idForIndex(index),
        batchId = batchId,
        sourcePath = path,
        sourceName = File(path).name,
    )
}

internal fun CreationNativeItem.isConfigurable(): Boolean =
    (!submitted && stage == CreationNativeStage.DRAFT) || stage.isTerminal()

internal fun CreationNativeUiState.submitSelectedItem(
    newItemId: String = "item_${UUID.randomUUID()}",
    newBatchId: String = "batch_${UUID.randomUUID()}",
    submissionToken: String = UUID.randomUUID().toString(),
): CreationNativeUiState {
    val selected = selectedItem ?: return this
    if (items.any { it.submissionToken == submissionToken }) return this
    val queued = selected.copy(
        id = newItemId,
        batchId = newBatchId,
        submitted = true,
        stage = CreationNativeStage.QUEUED,
        status = null,
        submissionToken = submissionToken,
        createdAtMs = System.currentTimeMillis(),
    )
    return copy(
        items = items + queued,
        selectedItemId = queued.id,
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

internal fun creationVisibleSessionSourceHandles(items: List<CreationNativeItem>): Set<String> =
    items.asSequence()
        .flatMap { (listOf(it.sourcePath) + it.referencePaths).asSequence() }
        .filter(String::isNotBlank)
        .toSet()
