package dev.screengoated.toolbox.mobile.creation

internal class CreationRecoveryWorkerLeases(
    private val acquireWorker: (CreationTool, String) -> Unit,
    private val releaseWorker: (CreationTool, String) -> Unit,
) {
    private val jobs = mutableSetOf<String>()

    constructor(workers: CreationWorkerPool) : this(workers::acquire, workers::release)

    fun acquire(jobId: String, tool: CreationTool) {
        val added = synchronized(jobs) { jobs.add(jobId) }
        if (added) acquireWorker(tool, leaseId(jobId))
    }

    fun release(jobId: String, tool: CreationTool?) {
        tool ?: return
        val removed = synchronized(jobs) { jobs.remove(jobId) }
        if (removed) releaseWorker(tool, leaseId(jobId))
    }

    private fun leaseId(jobId: String) = "recovery:$jobId"
}
