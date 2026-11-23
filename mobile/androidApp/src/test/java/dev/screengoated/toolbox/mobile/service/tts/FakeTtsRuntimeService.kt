package dev.screengoated.toolbox.mobile.service.tts

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicLong

internal class FakeTtsRuntimeService : TtsRuntimeService {
    private val mutableRuntimeState = MutableStateFlow(TtsRuntimeState())
    private val mutablePlaybackEvents = MutableSharedFlow<TtsPlaybackEvent>(extraBufferCapacity = 16)
    private val mutableEdgeCatalogState = MutableStateFlow(EdgeVoiceCatalogState())
    private val nextRequestId = AtomicLong(1)
    private val interruptions = TtsInterruptionTracker()
    var beforeRealtimeReturn: ((Long, TtsRequest) -> Unit)? = null

    data class RecordedRequest(
        val id: Long,
        val text: String,
    )

    val enqueuedRealtime = CopyOnWriteArrayList<RecordedRequest>()
    val enqueuedRequests = CopyOnWriteArrayList<TtsRequest>()
    val stopCount = java.util.concurrent.atomic.AtomicInteger()
    val rejectionsRemaining = java.util.concurrent.atomic.AtomicInteger()
    val cancelledAdmissions = java.util.concurrent.atomic.AtomicInteger()
    val realtimeDepths = CopyOnWriteArrayList<Int>()
    val retiredRealtime = CopyOnWriteArrayList<Pair<Long, String>>()

    override val runtimeState: StateFlow<TtsRuntimeState> = mutableRuntimeState
    override fun playbackEventsFor(vararg selectors: TtsEventSelector): Flow<TtsPlaybackEvent> =
        mutablePlaybackEvents.filter { event -> selectors.any { it.matches(event.consumer, event.ownerToken) } }
    override val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState> = mutableEdgeCatalogState

    override fun ensureEdgeVoiceCatalog(force: Boolean) = Unit

    override fun enqueue(request: TtsRequest): Long {
        return nextRequestId.getAndIncrement()
    }

    override fun enqueueRealtime(request: TtsRequest): Long {
        if (request.resumeAfterInterruption?.let { !interruptions.resumeAllowed(it) } == true) {
            cancelledAdmissions.incrementAndGet()
            return TTS_REQUEST_CANCELLED
        }
        if (rejectionsRemaining.getAndUpdate { if (it > 0) it - 1 else it } > 0) return TTS_REQUEST_REJECTED
        val requestId = nextRequestId.getAndIncrement()
        check(interruptions.register(requestId))
        enqueuedRealtime += RecordedRequest(requestId, request.text)
        enqueuedRequests += request
        beforeRealtimeReturn?.invoke(requestId, request)
        return requestId
    }

    override fun realtimeContinuationFence(): TtsInterruption = interruptions.continuationFence()

    override fun interruptAndSpeak(request: TtsRequest): Long {
        val id = nextRequestId.getAndIncrement()
        interruptions.interrupt(id)
        check(interruptions.register(id))
        return id
    }

    override fun enqueueSettingsPreview(request: TtsRequest): Long = enqueue(request)

    override fun stop() { interruptions.interrupt(null); stopCount.incrementAndGet() }

    override fun stopIfActive(requestId: Long) = Unit

    override fun retireRealtime(requestId: Long, ownerToken: String) {
        retiredRealtime += requestId to ownerToken
    }

    override fun hasPendingAudio(): Boolean = false

    override fun setRealtimeQueueDepth(depth: Int) {
        realtimeDepths += depth
    }

    fun emitPlaybackEvent(event: TtsPlaybackEvent) {
        emitRecordedEvent(terminalEvent(event))
    }

    fun terminalEvent(event: TtsPlaybackEvent): TtsPlaybackEvent {
        nextRequestId.updateAndGet { maxOf(it, event.requestId + 1) }
        val cause = interruptions.terminal(event.requestId)
        return event.copy(interruptedBy = event.interruptedBy ?: cause)
    }

    fun emitRecordedEvent(event: TtsPlaybackEvent) { mutablePlaybackEvents.tryEmit(event) }

    suspend fun awaitPlaybackSubscriber() {
        withTimeout(5_000) {
            mutablePlaybackEvents.subscriptionCount.first { it > 0 }
        }
    }

    suspend fun awaitNoPlaybackSubscriber() {
        withTimeout(5_000) { mutablePlaybackEvents.subscriptionCount.first { it == 0 } }
    }

    suspend fun awaitRealtimeDepth(expected: Int) {
        withTimeout(5_000) {
            while (realtimeDepths.lastOrNull() != expected) {
                delay(5)
            }
        }
    }
}
