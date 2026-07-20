package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlOverlayControllerTest {
    @Test
    fun visualOnlyUpdatesDoNotInvalidateWindowLayout() {
        assertFalse(
            needsOverlayLayoutUpdate(
                forceLayout = false,
                windowSetChanged = false,
                suppressionChanged = false,
            ),
        )
        assertTrue(needsOverlayLayoutUpdate(true, false, false))
        assertTrue(needsOverlayLayoutUpdate(false, true, false))
        assertTrue(needsOverlayLayoutUpdate(false, false, true))
    }

    @Test
    fun exclusionMarksOnlyTheOverlayTransitionScope() = runTest {
        val participant = object : PhoneControlOverlayExclusionParticipant {
            override suspend fun <T> withOverlayHidden(block: suspend () -> T): T = block()

            override fun orbBounds() = OverlayBounds(0, 0, 10, 10)
        }
        PhoneControlOverlayExclusion.register(participant)
        try {
            assertFalse(PhoneControlOverlayExclusion.controllerTransitionActive)
            PhoneControlOverlayExclusion.forCapture {
                assertTrue(PhoneControlOverlayExclusion.controllerTransitionActive)
            }
            assertFalse(PhoneControlOverlayExclusion.controllerTransitionActive)
        } finally {
            PhoneControlOverlayExclusion.unregister(participant)
        }
    }

    @Test
    fun actionAvoidanceChoosesTheFarthestValidCorner() {
        assertEquals(
            12 to 12,
            farthestOverlayCorner(
                screen = OverlayBounds(0, 0, 1_000, 2_000),
                overlayWidth = 100,
                overlayHeight = 100,
                margin = 12,
                avoid = OverlayBounds(850, 1_700, 950, 1_900),
            ),
        )
    }

    @Test
    fun pointerExclusionRelocatesInsteadOfHidingWhenSupported() = runTest {
        var relocated = false
        var hidden = false
        val participant = object : PhoneControlOverlayExclusionParticipant {
            override suspend fun <T> withOverlayHidden(block: suspend () -> T): T {
                hidden = true
                return block()
            }

            override suspend fun <T> withOverlayAvoiding(
                bounds: OverlayBounds,
                block: suspend () -> T,
            ): T {
                relocated = 5 >= bounds.left && 5 < bounds.right &&
                    5 >= bounds.top && 5 < bounds.bottom
                return block()
            }

            override fun orbBounds() = OverlayBounds(0, 0, 10, 10)
        }
        PhoneControlOverlayExclusion.register(participant)
        try {
            PhoneControlOverlayExclusion.forPoint(5f, 5f) {}
            assertTrue(relocated)
            assertFalse(hidden)
        } finally {
            PhoneControlOverlayExclusion.unregister(participant)
        }
    }

    @Test
    fun cancellationDuringPreBodyHideAlwaysRestoresTheGate() = runTest {
        val gate = OverlayCaptureGate()
        val hideStarted = CompletableDeferred<Unit>()
        var bodyRan = false
        var restoreRan = false

        val capture = launch {
            gate.withHidden(
                onHide = {
                    hideStarted.complete(Unit)
                    awaitCancellation()
                },
                onRestore = { lastCapture ->
                    yield()
                    restoreRan = lastCapture
                },
                block = {
                    bodyRan = true
                },
            )
        }

        hideStarted.await()
        assertEquals(1, gate.depth)
        capture.cancelAndJoin()

        assertEquals(0, gate.depth)
        assertFalse(gate.isHidden)
        assertFalse(bodyRan)
        assertTrue(restoreRan)
    }

    @Test
    fun cancelledWaiterNeverDecrementsAnotherCapture() = runTest {
        val gate = OverlayCaptureGate()
        val firstHideStarted = CompletableDeferred<Unit>()
        val releaseFirstHide = CompletableDeferred<Unit>()

        val first = launch {
            gate.withHidden(
                onHide = {
                    firstHideStarted.complete(Unit)
                    releaseFirstHide.await()
                },
                onRestore = {},
                block = {},
            )
        }
        firstHideStarted.await()
        val cancelledWaiter = launch {
            gate.withHidden(onHide = {}, onRestore = {}, block = {})
        }
        yield()

        cancelledWaiter.cancelAndJoin()
        assertEquals(1, gate.depth)
        assertTrue(gate.isHidden)

        releaseFirstHide.complete(Unit)
        first.join()
        assertEquals(0, gate.depth)
        assertFalse(gate.isHidden)
    }

    @Test
    fun exceptionDuringPreBodyHideAlwaysRestoresTheGate() = runTest {
        val gate = OverlayCaptureGate()
        var restoreRan = false

        var failure: Throwable? = null
        try {
            gate.withHidden(
                onHide = { error("hide failed") },
                onRestore = { restoreRan = it },
                block = { error("body must not run") },
            )
        } catch (error: Throwable) {
            failure = error
        }

        assertTrue(failure is IllegalStateException)
        assertEquals("hide failed", failure.message)
        assertEquals(0, gate.depth)
        assertFalse(gate.isHidden)
        assertTrue(restoreRan)
    }

    @Test
    fun bodyExceptionAlsoRestoresTheGate() = runTest {
        val gate = OverlayCaptureGate()

        var failure: Throwable? = null
        try {
            gate.withHidden(
                onHide = {},
                onRestore = {},
                block = { error("body failed") },
            )
        } catch (error: Throwable) {
            failure = error
        }

        assertTrue(failure is IllegalStateException)
        assertEquals("body failed", failure.message)
        assertEquals(0, gate.depth)
        assertFalse(gate.isHidden)
    }

    @Test
    fun nestedCapturesRestoreOnlyAfterTheLastScopeLeaves() = runTest {
        val gate = OverlayCaptureGate()
        val transitions = mutableListOf<String>()

        gate.withHidden(
            onHide = { transitions += "hide:$it:${gate.depth}" },
            onRestore = { transitions += "restore:$it:${gate.depth}" },
        ) {
            assertEquals(1, gate.depth)
            gate.withHidden(
                onHide = { transitions += "hide:$it:${gate.depth}" },
                onRestore = { transitions += "restore:$it:${gate.depth}" },
            ) {
                assertEquals(2, gate.depth)
                assertTrue(gate.isHidden)
            }
            assertEquals(1, gate.depth)
            assertTrue(gate.isHidden)
        }

        assertEquals(
            listOf(
                "hide:true:1",
                "hide:false:2",
                "restore:false:1",
                "restore:true:0",
            ),
            transitions,
        )
        assertEquals(0, gate.depth)
        assertFalse(gate.isHidden)
    }
}
