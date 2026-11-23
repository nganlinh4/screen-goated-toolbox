package dev.screengoated.toolbox.mobile.service

/** Thread-safe bounded FIFO; overflow discards the oldest unsubmitted samples. */
internal class PendingLiveAudio(private val capacity: Int = MAX_PENDING_LIVE_SAMPLES) {
    init { require(capacity > 0) }
    private val samples = ShortArray(capacity)
    private var head = 0
    private var size = 0

    @Synchronized
    fun append(input: ShortArray): Int {
        var dropped = 0
        for (sample in input) {
            if (size == capacity) {
                head = (head + 1) % capacity
                size--
                dropped++
            }
            samples[(head + size) % capacity] = sample
            size++
        }
        return dropped
    }

    @Synchronized
    fun prepend(input: ShortArray): Int {
        val dropped = (size.toLong() + input.size - capacity).coerceAtLeast(0).toInt()
        val skip = minOf(dropped, input.size)
        repeat(dropped - skip) { head = (head + 1) % capacity; size-- }
        for (index in input.lastIndex downTo skip) {
            head = (head - 1 + capacity) % capacity
            samples[head] = input[index]
            size++
        }
        return dropped
    }

    @Synchronized
    fun take(limit: Int): ShortArray {
        val count = minOf(size, limit.coerceAtLeast(0))
        return ShortArray(count) {
            samples[head].also { head = (head + 1) % capacity; size-- }
        }
    }

    @Synchronized
    fun isEmpty(): Boolean = size == 0
}

internal const val MAX_PENDING_LIVE_SAMPLES = 16_000 * 60

/** Returns the end of the first frame whose delivery is uncertain, or null on success. */
internal fun sendLiveAudioFrames(samples: ShortArray, chunkSize: Int, send: (ShortArray) -> Boolean): Int? {
    require(chunkSize > 0)
    var offset = 0
    while (offset < samples.size) {
        val end = minOf(offset + chunkSize, samples.size)
        if (!send(samples.copyOfRange(offset, end))) return end
        offset = end
    }
    return null
}
