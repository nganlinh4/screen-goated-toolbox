package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetSessionOwnership
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread

class InterruptiblePlaybackTest {
    @Test
    fun playbackEndRequiresActualAudioAndPreservesPartialOutcome() {
        var drained = false
        var stopped = false
        assertEquals(TtsCompletionStatus.FAILED, finishTtsPlayback(false, { drained = true; PlaybackDrainResult.COMPLETE }, { stopped = true }))
        assertTrue(stopped)
        assertFalse(drained)
        assertEquals(TtsCompletionStatus.INTERRUPTED, finishTtsPlayback(false, { error("No audio to drain") }, {}, { false }))
        assertEquals(TtsCompletionStatus.PARTIAL, finishTtsPlayback(true, { PlaybackDrainResult.TIMED_OUT }, { error("unexpected stop") }))
        assertEquals(TtsCompletionStatus.PARTIAL, finishTtsPlayback(true, { PlaybackDrainResult.INTERRUPTED }, { error("unexpected stop") }))
        assertEquals(TtsCompletionStatus.COMPLETED, finishTtsPlayback(true, { PlaybackDrainResult.COMPLETE }, { error("unexpected stop") }))
    }

    @Test
    fun retirementBetweenWriteIntentAndDispatchPreventsTheWrite() {
        val ownership = PresetSessionOwnership(MutableStateFlow(PresetExecutionState(sessionId = "first", isExecuting = true)))
        val lease = ownership.effectLease("first")
        val beforeDispatch = CountDownLatch(1)
        val resumeDispatch = CountDownLatch(1)
        val result = AtomicReference<PlaybackWriteResult>()
        var writes = 0
        val worker = thread {
            result.set(writeInterruptibly(4_000, lease::isCurrent, { effect ->
                beforeDispatch.countDown()
                check(resumeDispatch.await(5, TimeUnit.SECONDS))
                lease.runIfCurrent(effect)
            }, { _, count ->
                writes++
                count
            }, {}))
        }
        assertTrue(beforeDispatch.await(5, TimeUnit.SECONDS))
        ownership.transition { ownership.retireEffects() }
        resumeDispatch.countDown()
        worker.join(5_000)
        assertFalse(worker.isAlive)
        assertEquals(PlaybackWriteResult.INTERRUPTED, result.get())
        assertEquals(0, writes)
    }

    @Test
    fun cancellationUnderBackpressureStopsBeforeAnotherWrite() {
        var current = true
        var writes = 0
        val result = writeInterruptibly(4_000, { current }, { effect -> if (current) { effect(); true } else false }, { _, _ -> writes++; 0 }, { current = false })
        assertEquals(PlaybackWriteResult.INTERRUPTED, result)
        assertEquals(1, writes)
    }

    @Test
    fun globalInterruptBeforeDispatchRejectsTheOldGeneration() {
        val gate = TtsDispatchGeneration()
        val previous = gate.current()
        gate.advance()
        var dispatched = false
        assertFalse(gate.runIfCurrent(previous) { dispatched = true; true })
        assertFalse(dispatched)
        assertTrue(gate.runIfCurrent(gate.current()) { dispatched = true; true })
        assertTrue(dispatched)
    }

    @Test
    fun presetEnqueueAndDispatchAcquireOwnershipBeforeGlobalGeneration() {
        val owner = PresetSessionOwnership(MutableStateFlow(PresetExecutionState(sessionId = "session", isExecuting = true)))
        val lease = owner.effectLease("session")
        val gate = TtsDispatchGeneration()
        val ownerHeld = CountDownLatch(1)
        val dispatchWaitingForOwner = CountDownLatch(1)
        val enqueueFinished = CountDownLatch(1)
        val request = TtsRequest("text", TtsConsumer.AUTO_SPEAK, TtsPriority.USER, TtsRequestMode.NORMAL,
            MobileGlobalTtsSettings().toRuntimeSnapshot(), "owner", lease::isCurrent,
            { effect -> dispatchWaitingForOwner.countDown(); lease.runIfCurrent(effect) },
        ).withDispatchGeneration(gate, gate.current())
        val enqueue = thread(isDaemon = true) {
            lease.runIfCurrent {
                ownerHeld.countDown()
                check(dispatchWaitingForOwner.await(5, TimeUnit.SECONDS))
                gate.current()
                enqueueFinished.countDown()
            }
        }
        assertTrue(ownerHeld.await(5, TimeUnit.SECONDS))
        val dispatch = thread(isDaemon = true) { request.dispatchIfCurrent { } }
        assertTrue("enqueue and dispatch deadlocked", enqueueFinished.await(5, TimeUnit.SECONDS))
        enqueue.join(5_000)
        dispatch.join(5_000)
        assertFalse(dispatch.isAlive)
    }

    @Test
    fun partialWritesCompleteAndDeviceErrorsFail() {
        val calls = mutableListOf<Pair<Int, Int>>()
        assertEquals(PlaybackWriteResult.COMPLETE, writeInterruptibly(4_000, { true }, { effect -> effect(); true }, { offset, count ->
            calls += offset to count
            minOf(count, 1_000)
        }, { error("unexpected wait") }))
        assertEquals(listOf(0, 1_000, 2_000, 3_000), calls.map { it.first })
        assertTrue(calls.all { it.second <= 1_920 })
        assertEquals(PlaybackWriteResult.FAILED, writeInterruptibly(2, { true }, { effect -> effect(); true }, { _, _ -> -1 }, {}))
    }

    @Test
    fun cancellationAfterAcceptedAudioIsPartialNotRetryableInterruption() {
        var current = true
        val result = writeInterruptibly(4_000, { current }, { effect -> effect(); true }, { _, count ->
            current = false
            count
        }, {})
        assertEquals(PlaybackWriteResult.PARTIAL, result)
        var calls = 0
        assertEquals(PlaybackWriteResult.PARTIAL, writeInterruptibly(4_000, { true }, { effect -> effect(); true }, { _, count ->
            if (calls++ == 0) count else -1
        }, {}))
    }

    @Test
    fun drainStopsOnRetirementAndCompletesNormallyForCurrentSession() {
        var time = 0L
        var current = true
        assertEquals(PlaybackDrainResult.INTERRUPTED, waitForPlaybackDrain(100, { time }, { current }, { false }, { time += 20; current = false }))
        time = 0
        assertEquals(PlaybackDrainResult.COMPLETE, waitForPlaybackDrain(100, { time }, { true }, { time >= 40 }, { time += 20 }))
        assertEquals(40L, time)
        time = 0
        assertEquals(PlaybackDrainResult.TIMED_OUT, waitForPlaybackDrain(100, { time }, { true }, { false }, { time += 20 }))
    }
}
