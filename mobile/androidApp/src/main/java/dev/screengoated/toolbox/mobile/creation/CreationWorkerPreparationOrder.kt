package dev.screengoated.toolbox.mobile.creation

internal data class CreationPreparationSlotState(
    val connected: Boolean,
    val binding: Boolean,
    val ready: Boolean,
    val busy: Boolean,
)

internal data class CreationPreparationDemand(
    val capacity: Int,
    val requiredWorkerKeys: Set<String>,
)

internal fun creationRequiredSlotIndexes(
    workerKeys: List<String>,
    requiredWorkerKeys: Set<String>,
): Set<Int> = requiredWorkerKeys.mapNotNullTo(sortedSetOf()) { key ->
    workerKeys.indexOf(key).takeIf { it >= 0 }
}

internal fun nextCreationPreparationSlot(
    slots: List<CreationPreparationSlotState>,
    requestedCapacity: Int,
    maximumConcurrentPreparations: Int,
    requiredSlots: Set<Int> = emptySet(),
): Int? {
    require(requestedCapacity in 1..slots.size)
    require(maximumConcurrentPreparations > 0)
    require(requiredSlots.all(slots.indices::contains))
    val activePreparations = slots.count {
        it.binding || (it.connected && !it.ready && !it.busy)
    }
    if (activePreparations >= maximumConcurrentPreparations) return null
    requiredSlots.sorted().firstOrNull { !slots[it].retained }?.let { required ->
        return required.takeUnless { slots[it].allocated }
    }
    if (slots.count(CreationPreparationSlotState::allocated) >= requestedCapacity) return null
    return slots.indexOfFirst {
        !it.connected && !it.binding && !it.ready
    }.takeIf { it >= 0 }
}

internal fun creationPreparationCapacitySatisfied(
    slots: List<CreationPreparationSlotState>,
    requestedCapacity: Int,
    requiredSlots: Set<Int> = emptySet(),
): Boolean {
    require(requestedCapacity in 0..slots.size)
    require(requiredSlots.all(slots.indices::contains))
    return requiredSlots.all { slots[it].retained } &&
        slots.count(CreationPreparationSlotState::retained) >= requestedCapacity
}

internal fun creationPreparationRetirementSlots(
    slots: List<CreationPreparationSlotState>,
    requestedCapacity: Int,
    requiredSlots: Set<Int> = emptySet(),
): List<Int> {
    require(requestedCapacity >= 0)
    require(requiredSlots.all(slots.indices::contains))
    val allocated = slots.withIndex().filter { it.value.allocated }
    val excess = (allocated.size - requestedCapacity).coerceAtLeast(0)
    return allocated.filterNot { it.value.busy || it.index in requiredSlots }
        .sortedWith(compareBy<IndexedValue<CreationPreparationSlotState>> { it.value.ready }
            .thenByDescending { it.index })
        .take(excess)
        .map(IndexedValue<CreationPreparationSlotState>::index)
}

private val CreationPreparationSlotState.allocated: Boolean
    get() = connected || binding || ready || busy

private val CreationPreparationSlotState.retained: Boolean
    get() = ready || busy

internal fun activeCreationPreparationAfterFailure(
    active: CreationTool?,
    failed: CreationTool,
): CreationTool? = active.takeUnless { it == failed }

internal fun hasIndependentPreparationLane(
    slots: List<CreationPreparationSlotState>,
    failedSlot: Int,
): Boolean = slots.withIndex().any { (index, slot) ->
    index != failedSlot && (slot.connected || slot.binding || slot.ready || slot.busy)
}

internal class CreationPreparationFailureRegistry {
    private val failed = mutableSetOf<CreationTool>()

    fun markFailed(tool: CreationTool): Boolean = failed.add(tool)

    fun markFailed(tools: Set<CreationTool>) {
        failed += tools
    }

    fun restart(tool: CreationTool): Boolean = failed.remove(tool)

    fun isFailed(tool: CreationTool): Boolean = tool in failed

    fun available(retained: Set<CreationTool>): Set<CreationTool> = retained - failed

    fun clear() = failed.clear()
}
