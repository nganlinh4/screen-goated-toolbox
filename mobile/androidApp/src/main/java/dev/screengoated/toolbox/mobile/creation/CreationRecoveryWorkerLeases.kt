package dev.screengoated.toolbox.mobile.creation

internal class CreationRecoveryWorkerLeases(
    private val workers: CreationWorkerPool,
) {
    private val jobs = mutableSetOf<String>()

    fun acquire(jobId: String, tool: CreationTool) {
        if (jobs.add(jobId)) workers.acquire(tool, leaseId(jobId))
    }

    fun release(jobId: String, tool: CreationTool?) {
        if (jobs.remove(jobId) && tool != null) workers.release(tool, leaseId(jobId))
    }

    private fun leaseId(jobId: String) = "recovery:$jobId"
}
