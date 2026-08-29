package dev.screengoated.toolbox.mobile.creation

internal class CreationRecoveryWorkerLeases(
    private val acquireWorker: (CreationTool, String, String?) -> Unit,
    private val requireWorker: (CreationTool, String, String) -> Unit,
    private val releaseWorker: (CreationTool, String) -> Unit,
) {
    private val jobs = mutableMapOf<String, RecoveryWorkerLease>()

    constructor(workers: CreationWorkerPool) : this(
        workers::acquireRecovery,
        workers::requireRecoveryWorker,
        workers::release,
    )

    fun acquire(jobId: String, tool: CreationTool, requiredWorkerKey: String? = null) {
        var added = false
        var requirementChanged = false
        synchronized(jobs) {
            val existing = jobs[jobId]
            require(existing == null || existing.tool == tool)
            if (existing == null) {
                jobs[jobId] = RecoveryWorkerLease(tool, requiredWorkerKey)
                added = true
            } else if (requiredWorkerKey != null &&
                existing.requiredWorkerKey != requiredWorkerKey
            ) {
                jobs[jobId] = existing.copy(requiredWorkerKey = requiredWorkerKey)
                requirementChanged = true
            }
        }
        when {
            added -> acquireWorker(tool, leaseId(jobId), requiredWorkerKey)
            requirementChanged -> requireWorker(tool, leaseId(jobId), requireNotNull(requiredWorkerKey))
        }
    }

    fun assign(jobId: String, tool: CreationTool, workerKey: String) =
        acquire(jobId, tool, workerKey)

    fun release(jobId: String, tool: CreationTool?) {
        val removed = synchronized(jobs) { jobs.remove(jobId) } ?: return
        require(tool == null || removed.tool == tool)
        releaseWorker(removed.tool, leaseId(jobId))
    }

    private fun leaseId(jobId: String) = "recovery:$jobId"
}

private data class RecoveryWorkerLease(
    val tool: CreationTool,
    val requiredWorkerKey: String?,
)
