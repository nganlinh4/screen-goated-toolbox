package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.model.withMethod
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RealtimeTtsCoordinatorTest {
    @Test
    fun `realtime skips existing history on first enable`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val committed = "Sentence one. Sentence two. Active sentence tail with more words"

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )

        delay(50)
        assertEquals(1, runtime.enqueuedRealtime.size)
        assertEquals(" Active sentence tail with more words", runtime.enqueuedRealtime.single().text)
    }

    @Test
    fun `interrupted realtime request requeues unread tail`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val committed = "Sentence one. Sentence two. Active sentence tail with more words"

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )
        delay(50)
        val first = runtime.enqueuedRealtime.single()
        runtime.emitPlaybackEvent(
            TtsPlaybackEvent(
                requestId = first.id,
                consumer = TtsConsumer.REALTIME,
                ownerToken = RealtimeTtsCoordinator.OWNER_TOKEN,
                completionStatus = TtsCompletionStatus.INTERRUPTED,
            ),
        )
        delay(50)

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )
        delay(50)

        assertEquals(2, runtime.enqueuedRealtime.size)
        assertEquals(" Active sentence tail with more words", runtime.enqueuedRealtime.last().text)
        assertTrue(runtime.realtimeDepths.last() >= 1)
    }

    @Test
    fun `google method coerces fast speed to normal`() {
        val settings = MobileGlobalTtsSettings(
            method = MobileTtsMethod.GEMINI_LIVE,
            speedPreset = MobileTtsSpeedPreset.FAST,
        )

        val next = settings.withMethod(MobileTtsMethod.GOOGLE_TRANSLATE)

        assertEquals(MobileTtsMethod.GOOGLE_TRANSLATE, next.method)
        assertEquals(MobileTtsSpeedPreset.NORMAL, next.speedPreset)
    }
}

private class FakeTtsRuntimeService : TtsRuntimeService {
    private val mutableRuntimeState = MutableStateFlow(TtsRuntimeState())
    private val mutablePlaybackEvents = MutableSharedFlow<TtsPlaybackEvent>(extraBufferCapacity = 16)
    private val mutableEdgeCatalogState = MutableStateFlow(EdgeVoiceCatalogState())
    private var nextRequestId = 1L

    data class RecordedRequest(
        val id: Long,
        val text: String,
    )

    val enqueuedRealtime = mutableListOf<RecordedRequest>()
    val realtimeDepths = mutableListOf<Int>()

    override val runtimeState: StateFlow<TtsRuntimeState> = mutableRuntimeState
    override val playbackEvents: SharedFlow<TtsPlaybackEvent> = mutablePlaybackEvents
    override val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState> = mutableEdgeCatalogState

    override fun ensureEdgeVoiceCatalog(force: Boolean) = Unit

    override fun enqueue(request: TtsRequest): Long {
        return nextRequestId++
    }

    override fun enqueueRealtime(request: TtsRequest): Long {
        val requestId = nextRequestId++
        enqueuedRealtime += RecordedRequest(requestId, request.text)
        return requestId
    }

    override fun interruptAndSpeak(request: TtsRequest): Long {
        return nextRequestId++
    }

    override fun stop() = Unit

    override fun stopIfActive(requestId: Long) = Unit

    override fun hasPendingAudio(): Boolean = false

    override fun setRealtimeQueueDepth(depth: Int) {
        realtimeDepths += depth
    }

    fun emitPlaybackEvent(event: TtsPlaybackEvent) {
        mutablePlaybackEvents.tryEmit(event)
    }
}
