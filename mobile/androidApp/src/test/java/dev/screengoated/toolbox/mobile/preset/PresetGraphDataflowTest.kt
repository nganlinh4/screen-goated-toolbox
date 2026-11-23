package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetGraphDataflowTest {
    @Test
    fun siblingBranchesRunConcurrently() = runTest {
        // 0 -> 1, 0 -> 2. Branch 1 waits for branch 2; serial execution would deadlock.
        val incoming = listOf(emptyList(), listOf(0), listOf(0))
        val secondStarted = CompletableDeferred<Unit>()
        val finished = mutableListOf<Int>()

        withTimeout(5_000) {
            runPresetDataflow(listOf(0, 1, 2), incoming) { index ->
                if (index == 1) secondStarted.await()
                if (index == 2) secondStarted.complete(Unit)
                finished += index
            }
        }

        assertEquals(listOf(0, 2, 1), finished)
    }

    @Test
    fun childWaitsForEveryParent() = runTest {
        // 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3.
        val incoming = listOf(emptyList(), listOf(0), listOf(0), listOf(1, 2))
        val slow = CompletableDeferred<Unit>()
        val finished = mutableListOf<Int>()

        runPresetDataflow(listOf(0, 1, 2, 3), incoming) { index ->
            if (index == 1) slow.await()
            if (index == 2) {
                finished += index
                slow.complete(Unit)
                return@runPresetDataflow
            }
            finished += index
        }

        assertEquals(3, finished.last())
        assertTrue(finished.indexOf(1) < finished.indexOf(3))
    }

    @Test
    fun failedOrBlankUpstreamSkipsTheBlock() {
        val running = setOf(0, 1, 2, 3)
        val adapter = { index: Int -> index == 0 }

        assertTrue(shouldSkipDownstreamBlock(listOf(1), running, mapOf(0 to "in", 1 to ""), adapter))
        assertTrue(shouldSkipDownstreamBlock(listOf(1, 2), running, mapOf(1 to " ", 2 to ""), adapter))
        assertTrue(shouldSkipDownstreamBlock(listOf(1), running, emptyMap(), adapter))
        assertFalse(shouldSkipDownstreamBlock(listOf(1, 2), running, mapOf(1 to "", 2 to "ok"), adapter))
    }

    @Test
    fun inputAdapterAndRootBlocksAlwaysRun() {
        val running = setOf(0, 1, 2)
        val adapter = { index: Int -> index == 0 }

        assertFalse(shouldSkipDownstreamBlock(listOf(0), running, mapOf(0 to ""), adapter))
        assertFalse(shouldSkipDownstreamBlock(emptyList(), running, emptyMap(), adapter))
        // A parent outside the reachable graph never runs and is ignored.
        assertFalse(shouldSkipDownstreamBlock(listOf(9), running, emptyMap(), adapter))
    }
}
