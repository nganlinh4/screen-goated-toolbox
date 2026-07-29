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
            var retiredInputs = emptyList<String>()
            val snapshot = synchronized(stateLock) {
                memory.jobs.replaceAll { _, status ->
                    if (status.outputPath == previous.outputPath) {
                        status.copy(outputPath = null, outputName = null, canSegment = false)
                    } else {
                        status
                    }
                }
                retiredInputs = memory.continuations.values
                    .filter { it.outputPath == previous.outputPath }
                    .map(CreationContinuation::sourcePath)
                memory.continuations.entries.removeAll {
                    it.value.outputPath == previous.outputPath
                }
                journalWriter.snapshot(memory)
            }
            journalWriter.writeRequired(snapshot)
            files.releaseJobInputs(retiredInputs)
            history.delete(id)
        }
    }

    fun deleteAll(tool: CreationTool) {
        history.list(tool).forEach { entry -> delete(tool, entry.id) }
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
