package dev.screengoated.toolbox.mobile.creation

internal class CreationManagerHistoryCoordinator(
    private val history: CreationHistoryStore,
    private val files: CreationFileStore,
    private val memory: CreationManagerMemory,
    private val mutationLock: Any,
    private val stateLock: Any,
    private val journalWriter: CreationManagerJournalWriter,
) {
    fun rename(tool: CreationTool, id: String, name: String): CreationHistoryEntry =
        synchronized(mutationLock) {
            val previous = history.list(tool).firstOrNull { it.id == id }
                ?: error("Result is no longer in history")
            val updated = history.rename(id, name)
            val snapshot = synchronized(stateLock) {
                remap(previous.outputPath, updated.outputPath, updated.outputName)
                journalWriter.snapshot(memory)
            }
            journalWriter.writeRequired(snapshot)
            updated
        }

    fun delete(tool: CreationTool, id: String) {
        synchronized(mutationLock) {
            val previous = history.list(tool).firstOrNull { it.id == id }
                ?: error("Result is no longer in history")
            // History first: if it cannot delete, jobs and continuations keep their links.
            history.delete(previous.id)
            detachDeletedOutputs(setOf(previous.outputPath))
        }
    }

    /**
     * Deletes every saved result of [tool] from one snapshot of the list, keeps going
     * past an entry that fails, detaches the deleted outputs with one journal write,
     * then reports the first failure (others suppressed) so the caller can refresh.
     */
    fun deleteAll(tool: CreationTool) {
        synchronized(mutationLock) {
            val batch = deleteCreationHistoryBatch(history.list(tool)) { entry ->
                history.delete(entry.id)
            }
            detachDeletedOutputs(batch.deleted.mapTo(mutableSetOf(), CreationHistoryEntry::outputPath))
            batch.throwFirstFailure()
        }
    }

    private fun detachDeletedOutputs(outputPaths: Set<String>) {
        if (outputPaths.isEmpty()) return
        var retiredInputs = emptyList<String>()
        val snapshot = synchronized(stateLock) {
            memory.jobs.replaceAll { _, status ->
                if (status.outputPath in outputPaths) {
                    status.copy(outputPath = null, outputName = null, canSegment = false)
                } else {
                    status
                }
            }
            retiredInputs = memory.continuations.values
                .filter { it.outputPath in outputPaths }
                .map(CreationContinuation::sourcePath)
            memory.continuations.entries.removeAll { it.value.outputPath in outputPaths }
            journalWriter.snapshot(memory)
        }
        journalWriter.writeRequired(snapshot)
        files.releaseJobInputs(retiredInputs)
    }

    fun reconcileAtStartup() {
        synchronized(mutationLock) {
            val byDispatch = CreationTool.entries
                .flatMap(history::list)
                .mapNotNull { entry -> entry.dispatchId?.let { it to entry } }
                .toMap()
            var changed = false
            val snapshot = synchronized(stateLock) {
                memory.requests.forEach { (jobId, request) ->
                    val entry = byDispatch[request.dispatchId] ?: return@forEach
                    val current = memory.jobs[jobId] ?: return@forEach
                    if (current.outputPath != entry.outputPath && current.stage == "done") {
                        memory.jobs[jobId] = current.copy(
                            outputPath = entry.outputPath,
                            outputName = entry.outputName,
                        )
                        memory.continuations[jobId]?.let { continuation ->
                            memory.continuations[jobId] = continuation.copy(
                                outputPath = entry.outputPath,
                                outputName = entry.outputName,
                            )
                        }
                        changed = true
                    }
                }
                journalWriter.snapshot(memory)
            }
            if (changed) journalWriter.writeRequired(snapshot)
        }
    }

    private fun remap(oldPath: String, newPath: String, newName: String) {
        memory.jobs.replaceAll { _, status ->
            if (status.outputPath == oldPath) {
                status.copy(outputPath = newPath, outputName = newName)
            } else {
                status
            }
        }
        memory.continuations.replaceAll { _, continuation ->
            if (continuation.outputPath == oldPath) {
                continuation.copy(outputPath = newPath, outputName = newName)
            } else {
                continuation
            }
        }
    }
}

internal class CreationHistoryBatchDeletion<T>(
    val deleted: List<T>,
    val failures: List<Throwable>,
) {
    fun throwFirstFailure() {
        val first = failures.firstOrNull() ?: return
        failures.drop(1).forEach(first::addSuppressed)
        throw first
    }
}

internal fun <T> deleteCreationHistoryBatch(
    entries: List<T>,
    delete: (T) -> Unit,
): CreationHistoryBatchDeletion<T> {
    val deleted = mutableListOf<T>()
    val failures = mutableListOf<Throwable>()
    entries.forEach { entry ->
        runCatching { delete(entry) }
            .onSuccess { deleted += entry }
            .onFailure { failures += it }
    }
    return CreationHistoryBatchDeletion(deleted, failures)
}
