package dev.screengoated.toolbox.mobile.creation

internal data class CreationWorkerAssignment(
    val jobId: String,
    val sink: (String, CreationWorkerEvent) -> Unit,
)

internal class CreationWorkerAssignmentGuard {
    private var active: CreationWorkerAssignment? = null

    val jobId: String?
        get() = active?.jobId

    fun claim(jobId: String, sink: (String, CreationWorkerEvent) -> Unit) {
        check(active == null) { "Creation worker already has an active assignment" }
        active = CreationWorkerAssignment(jobId, sink)
    }

    fun owns(jobId: String, sink: (String, CreationWorkerEvent) -> Unit): Boolean =
        active?.let { it.jobId == jobId && it.sink === sink } == true

    fun release(jobId: String): CreationWorkerAssignment? {
        val assignment = active?.takeIf { it.jobId == jobId } ?: return null
        active = null
        return assignment
    }

    fun lose(): CreationWorkerAssignment? = active.also { active = null }
}
