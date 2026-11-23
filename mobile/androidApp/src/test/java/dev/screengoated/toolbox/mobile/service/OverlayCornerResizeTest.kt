package dev.screengoated.toolbox.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class OverlayCornerResizeTest {
    @Test
    fun windowBeyondScreenAfterRotation_doesNotThrow() {
        // Portrait position kept after rotating to a narrower/shorter screen.
        val current = OverlayBounds(x = 900, y = 1800, width = 300, height = 400)
        val next = resize(current, "br", dx = 40, dy = 40, screenWidth = 800, screenHeight = 1200)
        assertEquals(MIN_W, next!!.width)
        assertEquals(MIN_H, next.height)
    }

    @Test
    fun paneBelowScreenAfterRotation_doesNotThrow() {
        val current = OverlayBounds(x = 100, y = 1500, width = 300, height = 400)
        val next = resize(current, "bl", dx = -20, dy = 10, screenWidth = 800, screenHeight = 1200)
        assertEquals(MIN_H, next!!.height)
        assertEquals(320, next.width)
        assertEquals(80, next.x)
    }

    @Test
    fun ordinaryResizeStaysInsideScreen() {
        val current = OverlayBounds(x = 100, y = 100, width = 300, height = 400)
        val grown = resize(current, "br", dx = 1000, dy = 1000, screenWidth = 800, screenHeight = 1200)
        assertEquals(OverlayBounds(x = 100, y = 100, width = 700, height = 1100), grown)
        val left = resize(current, "bl", dx = 50, dy = 0, screenWidth = 800, screenHeight = 1200)
        assertEquals(OverlayBounds(x = 150, y = 100, width = 250, height = 400), left)
    }

    @Test
    fun unknownCorner_returnsNull() {
        assertNull(resize(OverlayBounds(0, 0, 300, 400), "tr", 1, 1, 800, 1200))
    }

    private fun resize(
        current: OverlayBounds,
        corner: String,
        dx: Int,
        dy: Int,
        screenWidth: Int,
        screenHeight: Int,
    ) = resizeOverlayFromCorner(current, corner, dx, dy, screenWidth, screenHeight, MIN_W, MIN_H)

    private companion object {
        const val MIN_W = 110
        const val MIN_H = 90
    }
}
