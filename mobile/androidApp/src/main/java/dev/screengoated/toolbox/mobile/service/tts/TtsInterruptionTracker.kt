package dev.screengoated.toolbox.mobile.service.tts

/** Continuation guard and, when present, the replacement that interrupted playback. */
data class TtsInterruption(val replacementRequestId: Long?, val stopEpoch: Long, val stopped: Boolean = false)

/** Tracks requests from admission to terminal publication, including the dequeue gap. */
internal class TtsInterruptionTracker(private val maxOutstanding: Int = 512) {
    init { require(maxOutstanding > 0) }

    private data class Entry(val stopEpoch: Long, var interrupted: Boolean = false, var cause: TtsInterruption? = null)

    private val lock = Any()
    private val requests = LinkedHashMap<Long, Entry>()
    private var stopEpoch = 0L

    fun register(requestId: Long): Boolean = synchronized(lock) {
        if (requestId <= 0 || requests.containsKey(requestId) || requests.size >= maxOutstanding) {
            return@synchronized false
        }
        requests[requestId] = Entry(stopEpoch)
        true
    }

    /** Call before invalidating the interrupted dispatch generation. */
    fun interrupt(replacementRequestId: Long?) = synchronized(lock) {
        if (replacementRequestId == null) stopEpoch++
        for ((id, entry) in requests) {
            if (id == replacementRequestId || entry.interrupted) continue
            entry.interrupted = true
            entry.cause = TtsInterruption(replacementRequestId, entry.stopEpoch, stopped = replacementRequestId == null)
        }
    }

    /** Removes the request; a second terminal cannot obtain a cause. */
    fun terminal(requestId: Long): TtsInterruption? = synchronized(lock) {
        requests.remove(requestId)?.let { it.cause ?: TtsInterruption(null, it.stopEpoch) }
    }

    /** Check on the runtime's serialized queue-control path before admitting a retry. */
    fun resumeAllowed(interruption: TtsInterruption): Boolean = synchronized(lock) {
        !interruption.stopped && interruption.stopEpoch == stopEpoch
    }

    fun continuationFence(): TtsInterruption = synchronized(lock) { TtsInterruption(null, stopEpoch) }
}
