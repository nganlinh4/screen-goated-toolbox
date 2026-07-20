package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleAdapter
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleConnection
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleState

internal typealias GeminiS2sLiveConnection = GeminiLiveLifecycleConnection

/** Continuous-stream compatibility wrapper over the shared lifecycle adapter. */
internal class GeminiS2sLiveLifecycleAdapter(
    clockMs: () -> Long,
    openConnectedSession: suspend () -> GeminiLiveConnectedSession,
    setupPayload: () -> String,
    onEffect: (GeminiLiveLifecycleEffect) -> Unit = {},
) {
    private val delegate = GeminiLiveLifecycleAdapter(
        policy = GeminiLiveLifecyclePolicy.continuous(),
        clockMs = clockMs,
        openConnectedSession = openConnectedSession,
        setupPayload = setupPayload,
        onEffect = onEffect,
    )

    val state: GeminiLiveLifecycleState
        get() = delegate.state

    val activeConnection: GeminiS2sLiveConnection?
        get() = delegate.activeConnection

    suspend fun ensureReady(): GeminiS2sLiveConnection? = delegate.ensureReady()

    fun inputSent(chunks: Long = 1) = delegate.inputSent(chunks)

    fun inputActivity() = delegate.inputActivity()

    fun updateWorkState(
        pendingWorkCount: Long,
        bufferedInputCount: Long,
        userSpeaking: Boolean,
    ) = delegate.updateWorkState(pendingWorkCount, bufferedInputCount, userSpeaking)

    suspend fun observeFrame(frame: GeminiLiveLifecycleFrame): List<GeminiLiveLifecycleEffect> =
        delegate.observeFrame(frame)

    suspend fun transportFailed(generation: Long): List<GeminiLiveLifecycleEffect> =
        delegate.transportFailed(generation)

    suspend fun serverError(
        generation: Long,
        retryable: Boolean,
        kind: String = "server",
    ): List<GeminiLiveLifecycleEffect> = delegate.serverError(generation, retryable, kind)

    suspend fun tick(): List<GeminiLiveLifecycleEffect> = delegate.tick()

    suspend fun cancel() = delegate.cancel()
}

internal class LiveTranslatePendingAudio(
    private val maxSamples: Int,
    private val frameSamples: Int,
) {
    private val chunks = ArrayDeque<ShortArray>()

    var sampleCount: Int = 0
        private set

    init {
        require(maxSamples >= frameSamples)
        require(frameSamples > 0)
    }

    fun append(chunk: ShortArray) {
        if (chunk.isEmpty()) return
        chunks.addLast(chunk.copyOf())
        sampleCount += chunk.size
        dropOldest((sampleCount - maxSamples).coerceAtLeast(0))
    }

    fun takeFirst(): ShortArray? {
        if (sampleCount < frameSamples) return null
        val frame = ShortArray(frameSamples)
        var outputOffset = 0
        while (outputOffset < frameSamples) {
            val first = chunks.removeFirst()
            val copied = minOf(first.size, frameSamples - outputOffset)
            first.copyInto(frame, destinationOffset = outputOffset, endIndex = copied)
            outputOffset += copied
            if (copied < first.size) {
                chunks.addFirst(first.copyOfRange(copied, first.size))
            }
        }
        sampleCount -= frameSamples
        return frame
    }

    fun restoreFirst(frame: ShortArray) {
        if (frame.isEmpty()) return
        require(frame.size == frameSamples)
        chunks.addFirst(frame.copyOf())
        sampleCount += frame.size
        dropNewest((sampleCount - maxSamples).coerceAtLeast(0))
    }

    private fun dropOldest(count: Int) {
        var remaining = count
        while (remaining > 0) {
            val first = chunks.removeFirst()
            val removed = minOf(remaining, first.size)
            remaining -= removed
            sampleCount -= removed
            if (removed < first.size) {
                chunks.addFirst(first.copyOfRange(removed, first.size))
            }
        }
    }

    private fun dropNewest(count: Int) {
        var remaining = count
        while (remaining > 0) {
            val last = chunks.removeLast()
            val removed = minOf(remaining, last.size)
            remaining -= removed
            sampleCount -= removed
            if (removed < last.size) {
                chunks.addLast(last.copyOfRange(0, last.size - removed))
            }
        }
    }
}
