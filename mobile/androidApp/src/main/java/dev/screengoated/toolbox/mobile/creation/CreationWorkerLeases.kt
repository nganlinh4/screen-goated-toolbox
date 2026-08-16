package dev.screengoated.toolbox.mobile.creation

internal class CreationWorkerLeaseRegistry {
    private val owners = CreationTool.entries.associateWith {
        mutableMapOf<String, CreationWorkerLease>()
    }

    fun acquire(
        tool: CreationTool,
        owner: String,
        kind: CreationWorkerLeaseKind,
        requiredWorkerKey: String? = null,
    ): Boolean {
        require(owner.isNotBlank())
        val leases = owners.getValue(tool)
        val wasEmpty = leases.isEmpty()
        val existing = leases[owner]
        require(existing == null || existing.kind == kind)
        leases[owner] = CreationWorkerLease(
            kind,
            requiredWorkerKey ?: existing?.requiredWorkerKey,
        )
        return wasEmpty && leases.isNotEmpty()
    }

    fun requireWorker(tool: CreationTool, owner: String, workerKey: String) {
        val leases = owners.getValue(tool)
        val existing = requireNotNull(leases[owner])
        require(existing.kind == CreationWorkerLeaseKind.JOB)
        leases[owner] = existing.copy(requiredWorkerKey = workerKey)
    }

    fun release(tool: CreationTool, owner: String): Boolean {
        val leases = owners.getValue(tool)
        leases -= owner
        return leases.isEmpty()
    }

    fun retained(tool: CreationTool): Boolean = owners.getValue(tool).isNotEmpty()

    fun retainedTools(): Set<CreationTool> = owners.filterValues { it.isNotEmpty() }.keys

    fun requestedCapacity(tool: CreationTool, minimum: Int, maximum: Int): Int {
        require(minimum in 1..maximum)
        val leases = owners.getValue(tool)
        if (leases.isEmpty()) return 0
        val jobs = leases.values.count { it.kind == CreationWorkerLeaseKind.JOB }
        return maxOf(minimum, jobs).coerceAtMost(maximum)
    }

    fun requiredWorkerKeys(tool: CreationTool): Set<String> = owners.getValue(tool).values
        .mapNotNullTo(mutableSetOf(), CreationWorkerLease::requiredWorkerKey)

    fun preparationDemand(tool: CreationTool) = CreationPreparationDemand(
        requestedCapacity(
            tool,
            CreationContract.minimumPreparedCapacity(tool),
            CreationContract.maximumParallelJobs(tool),
        ),
        requiredWorkerKeys(tool),
    )

    fun clear() = owners.values.forEach { it.clear() }
}

internal enum class CreationWorkerLeaseKind { SURFACE, JOB }

private data class CreationWorkerLease(
    val kind: CreationWorkerLeaseKind,
    val requiredWorkerKey: String?,
)

internal fun selectCreationPreparationTool(
    active: CreationTool?,
    retained: Set<CreationTool>,
    ready: Set<CreationTool>,
    surfacePriority: List<CreationTool>,
): CreationTool? {
    return surfacePriority.firstOrNull { it in retained && it !in ready }
        ?: active?.takeIf { it in retained && it !in ready }
        ?: retained.firstOrNull { it !in ready }
}
