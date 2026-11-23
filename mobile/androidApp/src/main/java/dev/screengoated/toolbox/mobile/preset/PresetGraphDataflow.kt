package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.Deferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope

/**
 * Runs preset blocks as a dataflow, as Windows `post_process.rs` does: a block
 * starts once every parent that runs has finished, so independent sibling
 * branches run concurrently. [executionOrder] must be topological.
 */
internal suspend fun runPresetDataflow(
    executionOrder: List<Int>,
    incoming: List<List<Int>>,
    run: suspend (index: Int) -> Unit,
) {
    coroutineScope {
        val finished = HashMap<Int, Deferred<Unit>>()
        executionOrder.forEach { index ->
            val parents = incoming[index].mapNotNull(finished::get)
            finished[index] = async {
                parents.awaitAll()
                run(index)
            }
        }
    }
}

/**
 * Windows stops a branch when a block yields no text (`should_continue`),
 * except after the input adapter. A block whose every running parent failed,
 * produced blank text or was itself skipped is therefore skipped too, instead
 * of running on an empty or substituted input.
 */
internal fun shouldSkipDownstreamBlock(
    parents: List<Int>,
    running: Set<Int>,
    outputs: Map<Int, String>,
    isInputAdapter: (Int) -> Boolean,
): Boolean {
    val ranParents = parents.filter(running::contains)
    if (ranParents.isEmpty() || ranParents.any(isInputAdapter)) return false
    return ranParents.all { parent -> outputs[parent].isNullOrBlank() }
}
