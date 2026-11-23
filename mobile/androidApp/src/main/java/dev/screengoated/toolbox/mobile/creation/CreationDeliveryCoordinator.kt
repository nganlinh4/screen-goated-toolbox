package dev.screengoated.toolbox.mobile.creation

internal enum class CreationDeliveryFailureAction {
    RETRY,
    FAIL_JOB,
}

internal fun creationDeliveryFailureAction(hasDurableReceipt: Boolean) =
    if (hasDurableReceipt) CreationDeliveryFailureAction.RETRY
    else CreationDeliveryFailureAction.FAIL_JOB

internal class CreationDeliveryCoordinator(
    private val deliveries: CreationDeliveryStore,
    private val finisher: CreationJobFinisher,
    private val journal: CreationJobJournal,
    private val journalWriter: CreationManagerJournalWriter,
    private val memory: CreationManagerMemory,
    private val dispatchQueue: CreationDispatchQueue,
    private val mutationLock: Any,
    private val stateLock: Any,
) {
    fun deliver(prepared: PreparedCreation, destination: String?): FinishedCreation {
        val receipt = deliveries.publish(deliveries.begin(prepared, destination))
        return finisher.completePublished(
            prepared,
            requireNotNull(receipt.publishedPath),
            receipt.intent.finalName,
            receipt.companion?.publishedPath,
            receipt.companion?.outputName,
        )
    }

    fun reconcileAtStartup(): Map<String, CreationTool> {
        synchronized(mutationLock) {
            val completed = deliveries.reconcile(finisher, liveArtifactPaths())
            if (completed.isEmpty()) return emptyMap()
            synchronized(stateLock) {
                completed.forEach(::completeJobLocked)
            }
            journal.save(snapshotCreationManagerState(memory))
            deliveries.finalizeJobs(completed.keys)
            return completed.mapValues { (_, terminal) ->
                requireNotNull(CreationTool.fromWireName(terminal.request.tool))
            }
        }
    }

    fun reconcileInProcess(): Map<String, CreationTool> {
        synchronized(mutationLock) {
            val completed = deliveries.reconcile(finisher, liveArtifactPaths())
            if (completed.isEmpty()) return emptyMap()
            val snapshot = synchronized(stateLock) {
                completed.forEach(::completeJobLocked)
                journalWriter.snapshot(memory)
            }
            journalWriter.writeRequired(snapshot)
            deliveries.finalizeJobs(completed.keys)
            return completed.mapValues { (_, terminal) ->
                requireNotNull(CreationTool.fromWireName(terminal.request.tool))
            }
        }
    }

    private fun liveArtifactPaths(): Set<String> =
        synchronized(stateLock) { memory.liveArtifactPaths() }

    private fun completeJobLocked(jobId: String, terminal: CreationReconciledTerminal) {
        dispatchQueue.remove(jobId)
        memory.jobs[jobId] = terminal.status
        terminal.continuation?.let { memory.continuations[jobId] = it }
            ?: memory.continuations.remove(jobId)
        memory.engineIds.remove(jobId)
    }
}
