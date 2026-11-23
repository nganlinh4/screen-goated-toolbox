package dev.screengoated.toolbox.mobile.service

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNull
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class LiveSessionStopOwnershipTest {
    @Test fun stoppedEpochCannotStartAfterItsDelay() = runTest {
        val owner = LiveSessionEpoch()
        val starting = owner.advance()
        var starts = 0
        backgroundScope.launch {
            delay(600)
            if (owner.isCurrent(starting)) starts++
        }
        runCurrent()
        owner.advance()
        advanceTimeBy(600)
        runCurrent()
        assertTrue(starts == 0)
    }

    @Test fun currentSessionIsRetiredAfterJoin() = runTest {
        val original = backgroundScope.launch { awaitCancellation() }
        runCurrent()
        var current: Job? = original
        var finished = false
        assertTrue(awaitLiveSessionStop(original, { current === it }) { current = null; finished = true })
        assertNull(current)
        assertTrue(finished)
    }

    @Test fun replacementInstalledDuringJoinIsNotCleared() = runTest {
        val finishing = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val original = backgroundScope.launch {
            try { awaitCancellation() } finally {
                withContext(NonCancellable) { finishing.complete(Unit); release.await() }
            }
        }
        runCurrent()
        var current: Job? = original
        val stop = backgroundScope.launch { awaitLiveSessionStop(original, { current === it }) { current = null } }
        runCurrent()
        assertTrue(finishing.isCompleted)
        val replacement = backgroundScope.launch { awaitCancellation() }
        current = replacement
        release.complete(Unit)
        runCurrent()
        assertTrue(stop.isCompleted)
        assertSame(replacement, current)
    }
}
