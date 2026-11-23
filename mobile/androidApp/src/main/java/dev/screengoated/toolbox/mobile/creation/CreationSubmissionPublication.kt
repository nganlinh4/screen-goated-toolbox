package dev.screengoated.toolbox.mobile.creation

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

internal fun publishCreationSubmission(
    state: MutableStateFlow<CreationNativeUiState>,
    expected: CreationNativeItem,
    routed: CreationNativeItem,
    status: CreationJobStatus,
): Boolean {
    while (true) {
        val current = state.value
        val index = current.items.indexOfFirst {
            it.id == expected.id && it.submissionToken == expected.submissionToken &&
                it.stage == CreationNativeStage.QUEUED && it.status?.jobId.isNullOrBlank()
        }
        if (index < 0) return false
        val items = current.items.toMutableList()
        items[index] = items[index].copy(
            stage = status.toNativeStage(), status = status,
            generationMode = status.generationMode ?: routed.generationMode,
            autoSegment = status.autoSegment ?: routed.autoSegment,
            segmentationLevel = status.segmentationLevel ?: routed.segmentationLevel,
        )
        if (state.compareAndSet(current, current.copy(items = items))) return true
    }
}

internal fun reconcileRetiredCreationSubmission(
    state: MutableStateFlow<CreationNativeUiState>,
    expected: CreationNativeItem,
    accepted: CreationJobStatus,
    cancel: (String) -> List<CreationJobStatus>,
    error: (Throwable) -> Unit,
) {
    val jobId = accepted.jobId?.takeIf(String::isNotBlank) ?: return
    val result = try {
        if (accepted.toNativeStage().isTerminal()) accepted else {
            cancel(jobId).firstOrNull { it.jobId == jobId }
                ?: throw IllegalStateException("Cancellation could not be confirmed. Try again.")
        }
    } catch (failure: Exception) {
        error(failure)
        accepted.copy(stage = "cancelling")
    }
    state.update { current ->
        current.copy(items = current.items.map { item ->
            if (item.id == expected.id && item.submissionToken == expected.submissionToken &&
                item.stage == CreationNativeStage.CANCELLED && item.status?.jobId.isNullOrBlank()) {
                item.copy(stage = result.toNativeStage(), status = result)
            } else item
        })
    }
}
