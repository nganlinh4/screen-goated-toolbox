package dev.screengoated.toolbox.mobile.creation

internal data class CreationPreparationSlotState(
    val connected: Boolean,
    val binding: Boolean,
    val ready: Boolean,
    val busy: Boolean,
)

internal fun nextCreationPreparationSlot(
    slots: List<CreationPreparationSlotState>,
): Int? = slots.indexOfFirst {
        !it.connected && !it.binding && !it.ready
    }.takeIf { it >= 0 }

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
