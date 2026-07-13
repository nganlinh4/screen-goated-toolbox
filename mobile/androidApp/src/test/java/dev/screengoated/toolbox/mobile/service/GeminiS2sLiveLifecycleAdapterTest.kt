package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionException
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionPhase
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

class GeminiS2sLiveLifecycleAdapterTest {
    @Test
    fun `adapter reconnects only on reducer deadline and resets after content`() = runTest {
        var nowMs = 0L
        val opened = mutableListOf<FakeConnectedSession>()
        val observedEffects = mutableListOf<GeminiLiveLifecycleEffect>()
        val adapter = GeminiS2sLiveLifecycleAdapter(
            clockMs = { nowMs },
            openConnectedSession = {
                FakeConnectedSession().also(opened::add)
            },
            setupPayload = { SETUP_PAYLOAD },
            onEffect = observedEffects::add,
        )

        val first = adapter.ensureReady()
        assertEquals(1L, first?.generation)
        assertEquals(GeminiLiveLifecyclePhase.ACTIVE, adapter.state.phase)
        assertEquals(1, opened.size)
        assertTrue(observedEffects[0] is GeminiLiveLifecycleEffect.OpenSocket)
        assertTrue(observedEffects[1] is GeminiLiveLifecycleEffect.SendSetup)

        adapter.inputSent(100)
        nowMs = 15_000
        adapter.tick()
        assertEquals(GeminiLiveLifecyclePhase.BACKING_OFF, adapter.state.phase)
        assertEquals(15_277L, adapter.state.reconnectDeadlineMs)
        assertTrue(opened.single().ready.closed)

        nowMs = 15_276
        assertNull(adapter.ensureReady())
        assertEquals(1, opened.size)
        nowMs = 15_277
        val second = adapter.ensureReady()
        assertEquals(2L, second?.generation)
        assertEquals(2, opened.size)
        assertEquals(1, adapter.state.reconnectAttempt)

        val featureEffects = adapter.observeFrame(
            GeminiLiveLifecycleFrame(generation = 2, contentCount = 1),
        )
        assertEquals(
            listOf(GeminiLiveLifecycleEffect.DeliverContent(1)),
            featureEffects,
        )
        assertEquals(0, adapter.state.reconnectAttempt)
    }

    @Test
    fun `adapter cancellation closes the active generation and absorbs future ticks`() = runTest {
        var nowMs = 0L
        val opened = mutableListOf<FakeConnectedSession>()
        val adapter = GeminiS2sLiveLifecycleAdapter(
            clockMs = { nowMs },
            openConnectedSession = { FakeConnectedSession().also(opened::add) },
            setupPayload = { SETUP_PAYLOAD },
        )

        val active = requireNotNull(adapter.ensureReady())
        adapter.cancel()

        assertTrue((active.session as FakeReadySession).closed)
        assertEquals(GeminiLiveLifecyclePhase.CANCELLED, adapter.state.phase)
        assertNull(adapter.activeConnection)
        nowMs = Long.MAX_VALUE
        assertNull(adapter.ensureReady())
        assertEquals(1, opened.size)
    }

    @Test
    fun `setup timeout is classified at the reducer deadline`() = runTest {
        var nowMs = 0L
        val observedEffects = mutableListOf<GeminiLiveLifecycleEffect>()
        val pending = FakeConnectedSession { _, _ ->
            nowMs = 14_999L
            throw GeminiLiveSessionException(GeminiLiveSessionFailure.SetupTimedOut)
        }
        val adapter = GeminiS2sLiveLifecycleAdapter(
            clockMs = { nowMs },
            openConnectedSession = { pending },
            setupPayload = { SETUP_PAYLOAD },
            onEffect = observedEffects::add,
        )

        assertNull(adapter.ensureReady())

        assertEquals(GeminiLiveLifecyclePhase.BACKING_OFF, adapter.state.phase)
        assertEquals(2L, adapter.state.generation)
        assertEquals(15_277L, adapter.state.reconnectDeadlineMs)
        assertTrue(pending.closed)
        assertEquals(
            listOf(
                GeminiLiveLifecycleEffect.OpenSocket(1),
                GeminiLiveLifecycleEffect.SendSetup(1),
                GeminiLiveLifecycleEffect.CloseSocket(1),
                GeminiLiveLifecycleEffect.ScheduleReconnect(
                    generation = 2,
                    attempt = 1,
                    delayMs = 277,
                    reason = dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReconnectReason.SETUP_TIMEOUT,
                ),
            ),
            observedEffects,
        )
    }

    @Test
    fun `retryable server error reconnects through the reducer`() = runTest {
        var nowMs = 100L
        val observedEffects = mutableListOf<GeminiLiveLifecycleEffect>()
        val opened = FakeConnectedSession()
        val adapter = GeminiS2sLiveLifecycleAdapter(
            clockMs = { nowMs },
            openConnectedSession = { opened },
            setupPayload = { SETUP_PAYLOAD },
            onEffect = observedEffects::add,
        )
        val active = requireNotNull(adapter.ensureReady())

        nowMs = 200L
        adapter.serverError(active.generation, retryable = true)

        assertEquals(GeminiLiveLifecyclePhase.BACKING_OFF, adapter.state.phase)
        assertEquals(2L, adapter.state.generation)
        assertTrue((active.session as FakeReadySession).closed)
        assertTrue(observedEffects.any {
            it is GeminiLiveLifecycleEffect.ScheduleReconnect &&
                it.reason == dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReconnectReason.SERVER_ERROR
        })
    }

    @Test
    fun `pending audio cap retains newest complete frames`() {
        val pending = LiveTranslatePendingAudio(maxSamples = 8, frameSamples = 4)

        pending.append(ShortArray(12) { it.toShort() })

        assertEquals(8, pending.sampleCount)
        val first = requireNotNull(pending.takeFirst())
        assertEquals(listOf<Short>(4, 5, 6, 7), first.toList())
        pending.restoreFirst(first)
        assertEquals(8, pending.sampleCount)
        assertEquals(listOf<Short>(4, 5, 6, 7), pending.takeFirst()?.toList())
        assertEquals(listOf<Short>(8, 9, 10, 11), pending.takeFirst()?.toList())
        assertEquals(0, pending.sampleCount)
    }

    @Test
    fun `pending audio coalesces split chunks before exposing a frame`() {
        val pending = LiveTranslatePendingAudio(maxSamples = 8, frameSamples = 4)

        pending.append(shortArrayOf(0, 1))
        assertNull(pending.takeFirst())
        assertEquals(2, pending.sampleCount)

        pending.append(shortArrayOf(2, 3, 4))
        assertEquals(listOf<Short>(0, 1, 2, 3), pending.takeFirst()?.toList())
        assertNull(pending.takeFirst())
        assertEquals(1, pending.sampleCount)

        pending.append(shortArrayOf(5, 6, 7))
        assertEquals(listOf<Short>(4, 5, 6, 7), pending.takeFirst()?.toList())
        assertEquals(0, pending.sampleCount)
    }

    @Test
    fun `combined content and interruption displays text before invalidating dequeued audio`() = runTest {
        val adapter = GeminiS2sLiveLifecycleAdapter(
            clockMs = { 0L },
            openConnectedSession = { FakeConnectedSession() },
            setupPayload = { SETUP_PAYLOAD },
        )
        val active = requireNotNull(adapter.ensureReady())
        val effects = adapter.observeFrame(
            GeminiLiveLifecycleFrame(
                generation = active.generation,
                contentCount = 1,
                interrupted = true,
            ),
        )
        assertEquals(
            listOf(
                GeminiLiveLifecycleEffect.DeliverContent(1),
                GeminiLiveLifecycleEffect.StopPlayback,
                GeminiLiveLifecycleEffect.DiscardQueuedOutput,
                GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration,
            ),
            effects,
        )

        val playbackEpoch = LiveTranslatePlaybackEpoch()
        val events = mutableListOf<String>()
        lateinit var dequeued: LiveTranslatePlaybackChunk
        executeLiveTranslateFeatureEffects(
            effects = effects,
            deliverContent = {
                events += "display"
                dequeued = playbackEpoch.tag(byteArrayOf(1, 2))
            },
            stopPlayback = {
                playbackEpoch.interrupt { events += "stop" }
            },
            discardQueuedOutput = { events += "discard" },
        )

        assertEquals(listOf("display", "stop", "discard"), events)
        assertFalse(playbackEpoch.playIfCurrent(dequeued) { events += "stale-play" })
        assertEquals(listOf("display", "stop", "discard"), events)
    }

    @Test
    fun `playback admitted before interruption is stopped after the blocking write`() {
        val playbackEpoch = LiveTranslatePlaybackEpoch()
        val chunk = playbackEpoch.tag(byteArrayOf(1))
        val events = Collections.synchronizedList(mutableListOf<String>())
        val playbackEntered = CountDownLatch(1)
        val releasePlayback = CountDownLatch(1)
        val interruptionStarted = CountDownLatch(1)

        val playbackThread = thread(name = "live-translate-playback-test") {
            assertTrue(
                playbackEpoch.playIfCurrent(chunk) {
                    events += "play-start"
                    playbackEntered.countDown()
                    assertTrue(releasePlayback.await(2, TimeUnit.SECONDS))
                    events += "play-end"
                },
            )
        }
        assertTrue(playbackEntered.await(2, TimeUnit.SECONDS))
        val interruptionThread = thread(name = "live-translate-interruption-test") {
            interruptionStarted.countDown()
            playbackEpoch.interrupt { events += "stop" }
        }
        assertTrue(interruptionStarted.await(2, TimeUnit.SECONDS))
        releasePlayback.countDown()
        playbackThread.join(2_000)
        interruptionThread.join(2_000)

        assertFalse(playbackThread.isAlive)
        assertFalse(interruptionThread.isAlive)
        assertEquals(listOf("play-start", "play-end", "stop"), events.toList())
    }

    @Test
    fun `continuous Live Translate rejects tool effects`() = runTest {
        val adapter = GeminiS2sLiveLifecycleAdapter(
            clockMs = { 0L },
            openConnectedSession = { FakeConnectedSession() },
            setupPayload = { SETUP_PAYLOAD },
        )
        val active = requireNotNull(adapter.ensureReady())
        val dispatch = adapter.observeFrame(
            GeminiLiveLifecycleFrame(
                generation = active.generation,
                toolCallIds = listOf("call"),
            ),
        )
        val cancellation = adapter.observeFrame(
            GeminiLiveLifecycleFrame(
                generation = active.generation,
                toolCancellationIds = listOf("call"),
            ),
        )

        listOf(dispatch, cancellation).forEach { effects ->
            val error = assertThrows(IllegalStateException::class.java) {
                executeLiveTranslateFeatureEffects(
                    effects = effects,
                    deliverContent = {},
                    stopPlayback = {},
                    discardQueuedOutput = {},
                )
            }
            assertEquals(
                "Live Translate does not support Gemini Live tool effects",
                error.message,
            )
        }
    }

    private class FakeReadySession : GeminiLiveReadySession {
        var closed: Boolean = false
            private set

        override val phase: GeminiLiveSessionPhase
            get() = if (closed) GeminiLiveSessionPhase.CLOSED else GeminiLiveSessionPhase.ACTIVE

        override fun trySend(payload: String): Boolean = !closed

        override suspend fun receive(timeoutMs: Long?): GeminiLiveReceiveResult {
            return GeminiLiveReceiveResult.TimedOut
        }

        override fun close() {
            closed = true
        }
    }

    private class FakeConnectedSession(
        private val activation: suspend (String, Long) -> GeminiLiveReadySession = { _, _ ->
            FakeReadySession()
        },
    ) : GeminiLiveConnectedSession {
        var closed = false
            private set
        lateinit var ready: FakeReadySession
            private set

        override val phase: GeminiLiveSessionPhase
            get() = if (closed) GeminiLiveSessionPhase.CLOSED else GeminiLiveSessionPhase.AWAITING_SETUP

        override suspend fun activate(
            setupPayload: String,
            timeoutMs: Long,
        ): GeminiLiveReadySession {
            assertEquals(SETUP_PAYLOAD, setupPayload)
            val activated = activation(setupPayload, timeoutMs)
            if (activated is FakeReadySession) ready = activated
            return activated
        }

        override fun close() {
            closed = true
        }
    }

    private companion object {
        private const val SETUP_PAYLOAD = """{"setup":{"model":"models/test"}}"""
    }
}
