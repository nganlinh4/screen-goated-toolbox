package dev.screengoated.toolbox.mobile.creation

import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

internal data class CreationManagerJournalSnapshot(
    val version: Long,
    val records: List<CreationJournalRecord>,
)

internal class CreationManagerJournalWriter(
    private val journal: CreationJobJournal,
    private val scope: CoroutineScope,
) {
    private val nextVersion = AtomicLong()
    private val writeLock = Any()
    private var committedVersion = -1L

    fun snapshot(memory: CreationManagerMemory) = CreationManagerJournalSnapshot(
        version = nextVersion.getAndIncrement(),
        records = snapshotCreationManagerState(memory),
    )

    fun schedule(snapshot: CreationManagerJournalSnapshot) {
        scope.launch { runCatching { write(snapshot) } }
    }

    fun writeRequired(snapshot: CreationManagerJournalSnapshot) {
        write(snapshot)
    }

    private fun write(snapshot: CreationManagerJournalSnapshot) {
        synchronized(writeLock) {
            if (snapshot.version <= committedVersion) return
            journal.save(snapshot.records)
            committedVersion = snapshot.version
        }
    }
}
