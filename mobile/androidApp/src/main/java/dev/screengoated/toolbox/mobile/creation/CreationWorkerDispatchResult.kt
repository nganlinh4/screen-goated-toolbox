package dev.screengoated.toolbox.mobile.creation

internal sealed interface CreationWorkerDispatchResult {
    data class Assigned(val workerKey: String) : CreationWorkerDispatchResult

    data object Waiting : CreationWorkerDispatchResult

    data object PreparationFailed : CreationWorkerDispatchResult
}
