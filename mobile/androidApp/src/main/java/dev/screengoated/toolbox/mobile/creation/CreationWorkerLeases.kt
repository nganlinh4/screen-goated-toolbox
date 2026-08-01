package dev.screengoated.toolbox.mobile.creation

internal class CreationWorkerLeaseRegistry {
    private val owners = CreationTool.entries.associateWith { mutableSetOf<String>() }

    fun acquire(tool: CreationTool, owner: String): Boolean {
        require(owner.isNotBlank())
        val leases = owners.getValue(tool)
        val wasEmpty = leases.isEmpty()
        leases += owner
        return wasEmpty && leases.isNotEmpty()
    }

    fun release(tool: CreationTool, owner: String): Boolean {
        val leases = owners.getValue(tool)
        leases -= owner
        return leases.isEmpty()
    }

    fun retained(tool: CreationTool): Boolean = owners.getValue(tool).isNotEmpty()

    fun retainedTools(): Set<CreationTool> = owners.filterValues { it.isNotEmpty() }.keys

    fun clear() = owners.values.forEach(MutableSet<String>::clear)
}

internal fun selectCreationPreparationTool(
    active: CreationTool?,
    retained: Set<CreationTool>,
    ready: Set<CreationTool>,
    surfacePriority: List<CreationTool>,
    startup: CreationTool?,
): CreationTool? {
    return surfacePriority.firstOrNull { it in retained && it !in ready }
        ?: active
        ?: startup?.takeIf { it in retained && it !in ready }
        ?: retained.firstOrNull { it !in ready }
}
