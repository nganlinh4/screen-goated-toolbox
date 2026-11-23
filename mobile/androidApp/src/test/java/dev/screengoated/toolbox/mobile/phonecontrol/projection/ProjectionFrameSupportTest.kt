package dev.screengoated.toolbox.mobile.phonecontrol.projection

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProjectionFrameSupportTest {
    @Test
    fun framesBetweenCapturesAreRetainedNotDropped() {
        assertEquals(
            ProjectionFrameDisposition.RETAIN,
            projectionFrameDisposition(active = true, captureWaiting = false, cacheEmpty = false),
        )
    }

    @Test
    fun waitingCaptureOrEmptyCacheDecodesAndRetiredReaderDrops() {
        assertEquals(
            ProjectionFrameDisposition.DECODE,
            projectionFrameDisposition(active = true, captureWaiting = true, cacheEmpty = false),
        )
        assertEquals(
            ProjectionFrameDisposition.DECODE,
            projectionFrameDisposition(active = true, captureWaiting = false, cacheEmpty = true),
        )
        assertEquals(
            ProjectionFrameDisposition.DROP,
            projectionFrameDisposition(active = false, captureWaiting = true, cacheEmpty = true),
        )
    }

    @Test
    fun rotationBelowApi34ResizesTheVirtualDisplay() {
        assertTrue(shouldFollowDisplayResize(29, 1080, 2400, 2400, 1080))
        assertTrue(shouldFollowDisplayResize(33, 2400, 1080, 1080, 2400))
        assertFalse(shouldFollowDisplayResize(33, 1080, 2400, 1080, 2400))
        assertFalse(shouldFollowDisplayResize(33, 1080, 2400, 0, 0))
    }

    @Test
    fun api34AndLaterLeaveResizingToThePlatformCallback() {
        assertFalse(shouldFollowDisplayResize(34, 1080, 2400, 2400, 1080))
        assertFalse(shouldFollowDisplayResize(36, 1080, 2400, 2400, 1080))
    }
}
