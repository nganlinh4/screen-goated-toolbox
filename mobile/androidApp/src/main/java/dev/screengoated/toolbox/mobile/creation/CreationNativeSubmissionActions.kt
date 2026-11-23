package dev.screengoated.toolbox.mobile.creation

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

internal fun submitCreationSelection(
    tool: CreationTool,
    state: MutableStateFlow<CreationNativeUiState>,
    preflight: CreationInputPreflight,
    error: (Throwable) -> Unit,
) {
    val current = state.value
    val selected = current.selectedItem ?: return
    if (tool == CreationTool.IMAGE_TO_3D && (selected.generationMode != CreationGenerationMode.QUALITY.wireName ||
        current.generationModes?.contains(selected.generationMode) == false)) {
        error(IllegalStateException("This generation mode is unavailable. Choose an available mode."))
        return
    }
    preflight.submit()
}

internal fun cancelCreationSelection(
    state: MutableStateFlow<CreationNativeUiState>,
    preflight: CreationInputPreflight,
    cancel: (String) -> List<CreationJobStatus>,
    error: (Throwable) -> Unit,
    changed: () -> Unit,
) {
    val selected = state.value.selectedItem ?: return
    if (preflight.cancelPending(selected.id) || selected.stage.isTerminal()) return
    val jobId = selected.status?.jobId
    if (selected.stage == CreationNativeStage.RUNNING && !jobId.isNullOrBlank()) {
        try {
            val result = cancel(jobId).firstOrNull { it.jobId == jobId }
                ?: throw IllegalStateException("Cancellation could not be confirmed. Try again.")
            state.update { current ->
                current.copy(items = current.items.map { item ->
                    if (item.id == selected.id && item.submissionToken == selected.submissionToken &&
                        item.status?.jobId == jobId && !item.stage.isTerminal()) {
                        item.copy(stage = result.toNativeStage(), status = result)
                    } else item
                })
            }
        } catch (failure: Exception) { error(failure) }
        changed()
        return
    }
    if (selected.stage != CreationNativeStage.QUEUED) return
    state.update { current ->
        current.copy(items = current.items.map { item ->
            if (item.id == selected.id && item.submissionToken == selected.submissionToken && item.stage == CreationNativeStage.QUEUED) {
                item.copy(stage = CreationNativeStage.CANCELLED, submitted = true)
            } else item
        })
    }
    changed()
}
