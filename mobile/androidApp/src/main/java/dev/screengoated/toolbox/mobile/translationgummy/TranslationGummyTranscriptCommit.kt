package dev.screengoated.toolbox.mobile.translationgummy

import java.util.concurrent.Executor
import kotlinx.coroutines.flow.MutableStateFlow

internal data class PreparedGummyTranscript(
    val expected: List<TranslationGummyTranscriptItem>,
    val updated: List<TranslationGummyTranscriptItem>,
    val index: Int,
)

/**
 * Publishes a transcript prepared (and language-classified) outside any lock, but only if
 * the transcript list is still the snapshot it was prepared from. A capture, session or
 * main-thread write in between makes the delayed result stale instead of overwriting it.
 */
internal fun commitPreparedGummyTranscript(
    state: MutableStateFlow<TranslationGummyState>,
    prepared: PreparedGummyTranscript,
    language: String,
    maxTranscripts: Int,
    normalize: (TranslationGummyState) -> TranslationGummyState,
): Boolean {
    val updated = prepared.updated.toMutableList()
    updated[prepared.index] = updated[prepared.index].copy(lang = language)
    while (true) {
        val current = state.value
        if (current.transcripts != prepared.expected) return false
        val next = normalize(current.copy(transcripts = updated.takeLast(maxTranscripts)))
        if (state.compareAndSet(current, next)) return true
    }
}

/**
 * Serialized, coalescing snapshot writer: writes run on [executor], a burst of requests
 * collapses into one write of the latest snapshot, and a failed write stays pending so the
 * next request retries it.
 */
internal class CoalescingSnapshotWriter<T>(
    private val executor: Executor,
    private val snapshot: () -> T,
    private val save: (T) -> Unit,
    private val onFailure: (Throwable) -> Unit,
) {
    private val lock = Any()
    private var dirty = false
    private var running = false

    fun request() {
        synchronized(lock) {
            dirty = true
            if (running) return
            running = true
        }
        executor.execute(::drain)
    }

    private fun drain() {
        while (true) {
            synchronized(lock) {
                if (!dirty) {
                    running = false
                    return
                }
                dirty = false
            }
            val error = runCatching { save(snapshot()) }.exceptionOrNull() ?: continue
            onFailure(error)
            val newerPending = synchronized(lock) {
                val newer = dirty
                dirty = true
                running = false
                newer
            }
            if (newerPending) request()
            return
        }
    }
}
