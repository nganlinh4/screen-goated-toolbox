package dev.screengoated.toolbox.mobile.creation

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CreationDispatchWakeTest {
    @Test
    fun idleRoundSleepsUntilTheFallbackInsteadOfPollingEverySecond() = runTest {
        val signal = Channel<Unit>(Channel.CONFLATED)
        awaitCreationDispatchWake(signal, 1_000, CREATION_DISPATCH_IDLE_FALLBACK_MS)
        assertEquals(CREATION_DISPATCH_IDLE_FALLBACK_MS, currentTime)
    }

    @Test
    fun workerReleaseSignalWakesAfterTheFloor() = runTest {
        val signal = Channel<Unit>(Channel.CONFLATED)
        var woke = false
        launch {
            awaitCreationDispatchWake(signal, 1_000, CREATION_DISPATCH_IDLE_FALLBACK_MS)
            woke = true
        }
        advanceTimeBy(3_000)
        runCurrent()
        assertFalse(woke)
        signal.trySend(Unit)
        runCurrent()
        assertTrue(woke)
        assertEquals(3_000L, currentTime)
    }

    @Test
    fun aSignalDuringTheRoundStillWakesButNoSoonerThanTheFloor() = runTest {
        val signal = Channel<Unit>(Channel.CONFLATED)
        signal.trySend(Unit)
        signal.trySend(Unit)
        awaitCreationDispatchWake(signal, 1_000, CREATION_DISPATCH_IDLE_FALLBACK_MS)
        assertEquals(1_000L, currentTime)
        assertTrue(signal.tryReceive().isFailure)
    }
}
