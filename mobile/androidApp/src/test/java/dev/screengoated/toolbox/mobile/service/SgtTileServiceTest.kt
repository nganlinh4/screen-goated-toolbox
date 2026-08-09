package dev.screengoated.toolbox.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Test

class SgtTileServiceTest {
    @Test
    fun runningBubbleAlwaysStopsRegardlessOfCurrentPermissionState() {
        assertEquals(BubbleTileAction.STOP, bubbleTileAction(true, true))
        assertEquals(BubbleTileAction.STOP, bubbleTileAction(true, false))
    }

    @Test
    fun missingOverlayPermissionRoutesToThePermissionFlow() {
        assertEquals(
            BubbleTileAction.REQUEST_OVERLAY_PERMISSION,
            bubbleTileAction(isRunning = false, canDrawOverlays = false),
        )
    }

    @Test
    fun readyBubbleStartsOnlyAfterPermissionIsPresent() {
        assertEquals(
            BubbleTileAction.START,
            bubbleTileAction(isRunning = false, canDrawOverlays = true),
        )
    }
}
