package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class WindowScopedCaptureMappingTest {
    @Test
    fun `absolute screen crop maps into a window scoped bitmap`() {
        val mapped = mapCaptureCrop(
            requested = TargetBounds(300, 500, 700, 900),
            capture = TargetBounds(100, 200, 900, 1_200),
            bitmapWidth = 800,
            bitmapHeight = 1_000,
        )

        assertEquals(TargetBounds(300, 500, 700, 900), mapped?.absoluteBounds)
        assertEquals(TargetBounds(200, 300, 600, 700), mapped?.bitmapBounds)
    }

    @Test
    fun `mapping preserves absolute identity while accounting for capture scaling`() {
        val mapped = mapCaptureCrop(
            requested = TargetBounds(200, 300, 600, 700),
            capture = TargetBounds(100, 100, 900, 900),
            bitmapWidth = 400,
            bitmapHeight = 400,
        )

        assertEquals(TargetBounds(200, 300, 600, 700), mapped?.absoluteBounds)
        assertEquals(TargetBounds(50, 100, 250, 300), mapped?.bitmapBounds)
    }

    @Test
    fun `mapping clips to capture and rejects disjoint requests`() {
        val clipped = mapCaptureCrop(
            requested = TargetBounds(0, 0, 250, 250),
            capture = TargetBounds(100, 100, 500, 500),
            bitmapWidth = 400,
            bitmapHeight = 400,
        )

        assertEquals(TargetBounds(100, 100, 250, 250), clipped?.absoluteBounds)
        assertEquals(TargetBounds(0, 0, 150, 150), clipped?.bitmapBounds)
        assertNull(
            mapCaptureCrop(
                requested = TargetBounds(0, 0, 50, 50),
                capture = TargetBounds(100, 100, 500, 500),
                bitmapWidth = 400,
                bitmapHeight = 400,
            ),
        )
    }
}
