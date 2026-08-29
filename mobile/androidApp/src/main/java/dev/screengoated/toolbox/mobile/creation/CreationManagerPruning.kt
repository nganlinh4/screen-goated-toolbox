package dev.screengoated.toolbox.mobile.creation

internal fun pruneCreationManagerTerminalDurably(
    memory: CreationManagerMemory,
    journalWriter: CreationManagerJournalWriter,
    files: CreationFileStore,
    stateLock: Any,
    nowMs: Long,
    continuationLifetimeMs: Long,
    maximumTerminalJobs: Int,
): Unit {
    lateinit var retirement: CreationMemoryRetirement
    val snapshot = synchronized(stateLock) {
        retirement = memory.pruneTerminal(
            nowMs,
            continuationLifetimeMs,
            maximumTerminalJobs,
        )
        journalWriter.snapshot(memory)
    }
    files.queueJobInputCleanup(retirement.retiredInputPaths)
    runCatching {
        journalWriter.writeRequired(snapshot)
    }.getOrElse {
        synchronized(stateLock) { retirement.rollback(memory) }
        return
    }
    files.drainJobInputCleanup()
}
