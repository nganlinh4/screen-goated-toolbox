package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive

internal data class CreationHistoryRetentionItem(
    val id: String,
    val tool: String,
    val createdAtMs: Long,
    val managedPaths: Set<String>,
)

internal fun planCreationHistoryRetention(
    entries: List<CreationHistoryRetentionItem>,
    maximumPerTool: Int,
    budgetBytes: Long,
    protectedManagedPaths: Set<String>,
    sizeOf: (String) -> Long,
): Set<String> {
    require(maximumPerTool > 0)
    require(budgetBytes >= 0L)
    val newestPerTool = entries.groupBy(CreationHistoryRetentionItem::tool)
        .values
        .mapNotNull { candidates -> candidates.maxByOrNull(CreationHistoryRetentionItem::createdAtMs) }
        .map(CreationHistoryRetentionItem::id)
        .toSet()
    val kept = entries.groupBy(CreationHistoryRetentionItem::tool)
        .values
        .flatMap { candidates ->
            candidates.sortedByDescending(CreationHistoryRetentionItem::createdAtMs)
                .take(maximumPerTool)
        }
        .associateBy(CreationHistoryRetentionItem::id)
        .toMutableMap()

    val references = mutableMapOf<String, Int>()
    protectedManagedPaths.forEach { references[it] = Int.MAX_VALUE }
    kept.values.flatMap { it.managedPaths }.forEach { path ->
        if (references[path] != Int.MAX_VALUE) {
            references[path] = (references[path] ?: 0) + 1
        }
    }
    val sizes = references.keys.associateWith { sizeOf(it).coerceAtLeast(0L) }
    var retainedBytes = sizes.values.fold(0L, ::saturatingCreationBytes)

    kept.values
        .filter { it.id !in newestPerTool }
        .sortedBy(CreationHistoryRetentionItem::createdAtMs)
        .forEach { candidate ->
            if (retainedBytes <= budgetBytes) return@forEach
            kept.remove(candidate.id)
            candidate.managedPaths.forEach pathLoop@ { path ->
                val count = references[path] ?: return@pathLoop
                if (count == Int.MAX_VALUE) return@pathLoop
                if (count <= 1) {
                    references.remove(path)
                    retainedBytes = (retainedBytes - sizes.getValue(path)).coerceAtLeast(0L)
                } else {
                    references[path] = count - 1
                }
            }
        }
    return kept.keys
}

internal fun creationHistoryAdmissionBudget(
    totalManagedBytes: Long,
    historyOwnedBytes: Long,
    globalBudgetBytes: Long,
): Long {
    require(totalManagedBytes >= 0L)
    require(historyOwnedBytes in 0..totalManagedBytes)
    require(globalBudgetBytes >= 0L)
    val nonHistoryBytes = totalManagedBytes - historyOwnedBytes
    return (globalBudgetBytes - nonHistoryBytes).coerceAtLeast(0L)
}

private fun saturatingCreationBytes(left: Long, right: Long): Long =
    if (right > Long.MAX_VALUE - left) Long.MAX_VALUE else left + right

internal fun CreationHistoryEntry.inputPaths(): Set<String> = buildSet {
    sourcePath.takeIf(String::isNotBlank)?.let(::add)
    (metadata["sourceImagePaths"] as? JsonArray)
        ?.mapNotNull { (it as? JsonPrimitive)?.content }
        ?.filter(String::isNotBlank)
        ?.let(::addAll)
    (metadata["referencePreviewPaths"] as? JsonArray)
        ?.mapNotNull { (it as? JsonPrimitive)?.content }
        ?.filter(String::isNotBlank)
        ?.let(::addAll)
    (metadata["sourcePreviewPath"] as? JsonPrimitive)
        ?.content
        ?.takeIf(String::isNotBlank)
        ?.let(::add)
}

internal fun CreationHistoryEntry.allPaths(): Set<String> = inputPaths() + outputPath
