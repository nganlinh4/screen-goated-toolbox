package dev.screengoated.toolbox.mobile.creation

internal sealed interface CreationWorkerDispatchResult {
    data class Assigned(val workerKey: String) : CreationWorkerDispatchResult

    data object Waiting : CreationWorkerDispatchResult

    data object TemporaryCapacityPause : CreationWorkerDispatchResult

    data object PreparationFailed : CreationWorkerDispatchResult
}

internal class CreationPreparationRecoveryWindow(
    private val durationMs: Long,
    private val nowMs: () -> Long = System::currentTimeMillis,
) {
    private val startedAt = mutableMapOf<String, Long>()

    init {
        require(durationMs > 0L)
    }

    fun shouldRetry(jobId: String, jobDeadlineAtMs: Long): Boolean {
        val now = nowMs()
        val started = startedAt.getOrPut(jobId) { now }
        val recoveryDeadline = runCatching { Math.addExact(started, durationMs) }
            .getOrDefault(Long.MAX_VALUE)
            .coerceAtMost(jobDeadlineAtMs)
        return now < recoveryDeadline
    }

    fun clear(jobId: String) {
        startedAt.remove(jobId)
    }

    fun retain(jobIds: Set<String>) {
        startedAt.keys.retainAll(jobIds)
    }
}
