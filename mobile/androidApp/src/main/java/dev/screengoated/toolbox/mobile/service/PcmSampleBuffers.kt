package dev.screengoated.toolbox.mobile.service

/** Growable primitive PCM16 buffer; audio hot paths never box samples. */
internal class ShortSampleBuffer(initialCapacity: Int = 16_384) {
    private var samples = ShortArray(initialCapacity.coerceAtLeast(1))

    var size: Int = 0
        private set

    fun add(sample: Short) {
        ensureCapacity(size + 1)
        samples[size++] = sample
    }

    fun addAll(chunk: ShortArray) {
        ensureCapacity(size + chunk.size)
        chunk.copyInto(samples, size)
        size += chunk.size
    }

    fun toShortArray(): ShortArray = samples.copyOf(size)

    private fun ensureCapacity(required: Int) {
        if (required <= samples.size) return
        samples = samples.copyOf(maxOf(required, samples.size * 2))
    }
}

/**
 * Converts captured PCM16 chunks into recognizer float batches. Both offline
 * ASR loops share it: a batch is released once [minSamples] are buffered, and
 * only the newest [maxSamples] are kept when the recognizer falls behind.
 */
internal class PcmFloatBatch(
    private val minSamples: Int,
    private val maxSamples: Int,
) {
    init {
        require(minSamples in 1..maxSamples) { "batch bounds must satisfy 1 <= min <= max" }
    }

    private var samples = FloatArray(minSamples * 2)
    private var size = 0

    @Synchronized
    fun append(chunk: ShortArray) {
        if (size + chunk.size > samples.size) {
            samples = samples.copyOf(maxOf(size + chunk.size, samples.size * 2))
        }
        for (sample in chunk) samples[size++] = sample / PCM16_SCALE
    }

    /** Returns the buffered batch, or null while fewer than [minSamples] are buffered. */
    @Synchronized
    fun take(): Taken? {
        if (size < minSamples) return null
        val dropped = (size - maxSamples).coerceAtLeast(0)
        val batch = samples.copyOfRange(dropped, size)
        size = 0
        if (samples.size > maxSamples * 2) samples = FloatArray(minSamples * 2)
        return Taken(batch, dropped)
    }

    class Taken(val samples: FloatArray, val droppedSamples: Int)

    private companion object {
        const val PCM16_SCALE = 32768f
    }
}

/** Growable primitive float buffer with front removal for streaming DSP. */
internal class FloatSampleBuffer(initialCapacity: Int = 8_192) {
    private var samples = FloatArray(initialCapacity.coerceAtLeast(1))

    var size: Int = 0
        private set

    val indices: IntRange
        get() = 0 until size

    fun isEmpty(): Boolean = size == 0

    operator fun get(index: Int): Float {
        if (index !in 0 until size) throw IndexOutOfBoundsException("index $index, size $size")
        return samples[index]
    }

    fun add(sample: Float) {
        ensureCapacity(size + 1)
        samples[size++] = sample
    }

    fun ensureCapacity(required: Int) {
        if (required <= samples.size) return
        samples = samples.copyOf(maxOf(required, samples.size * 2))
    }

    /** Removes the first [count] samples, keeping the rest in order. */
    fun dropFirst(count: Int) {
        val removed = count.coerceIn(0, size)
        samples.copyInto(samples, destinationOffset = 0, startIndex = removed, endIndex = size)
        size -= removed
    }

    fun clear() {
        size = 0
    }
}
