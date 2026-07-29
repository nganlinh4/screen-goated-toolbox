package dev.screengoated.toolbox.mobile.creation

internal data class CreationPendingDispatch(
    val jobId: String,
    val tool: CreationTool,
    val preferredEngineId: String? = null,
)

internal class CreationDispatchQueue(
    private val maximumQueuedPerTool: Int,
) {
    private val entries = mutableListOf<CreationPendingDispatch>()

    fun offer(pending: CreationPendingDispatch): Boolean {
        if (entries.any { it.jobId == pending.jobId }) return true
        if (entries.count { it.tool == pending.tool } >= maximumQueuedPerTool) return false
        entries += pending
        return true
    }

    fun remove(jobId: String): Boolean = entries.removeAll { it.jobId == jobId }

    fun snapshot(): List<CreationPendingDispatch> = entries.toList()

    fun count(tool: CreationTool): Int = entries.count { it.tool == tool }
}

internal class CreationWorkerEventBuffer {
    private val terminal = ArrayDeque<CreationWorkerEnvelope>()
    private val terminalJobs = mutableSetOf<String>()
    private val progress = linkedMapOf<String, CreationWorkerEnvelope>()

    fun offer(envelope: CreationWorkerEnvelope) {
        val jobId = envelope.event.jobId ?: return
        if (envelope.event.isControlEvent()) {
            progress.remove(jobId)
            if (terminalJobs.add(jobId)) terminal.addLast(envelope)
        } else if (jobId !in terminalJobs) {
            progress[jobId] = envelope
        }
    }

    fun poll(): CreationWorkerEnvelope? {
        if (terminal.isNotEmpty()) {
            return terminal.removeFirst().also { terminalJobs.remove(it.event.jobId) }
        }
        val first = progress.entries.firstOrNull() ?: return null
        progress.remove(first.key)
        return first.value
    }

    fun size(): Int = terminal.size + progress.size
}

private fun CreationWorkerEvent.isControlEvent(): Boolean = event in setOf(
    "success",
    "failure",
    "cancelled",
    "execution_lost",
)

internal sealed interface CreationSubmissionOutcome {
    data class Accepted(val status: CreationJobStatus) : CreationSubmissionOutcome
    data class Rejected(val category: CreationSubmissionFailure) : CreationSubmissionOutcome
}

internal enum class CreationSubmissionFailure {
    INVALID_REQUEST,
    QUEUE_FULL,
    SOURCE_UNAVAILABLE,
    STORAGE_UNAVAILABLE,
}
