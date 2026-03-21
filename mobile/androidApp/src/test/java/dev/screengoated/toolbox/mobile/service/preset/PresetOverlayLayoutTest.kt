package dev.screengoated.toolbox.mobile.service.preset

import android.graphics.Rect
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import kotlin.math.roundToInt
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetOverlayLayoutTest {
    private val density = 2f
    private val screenBounds = Rect(0, 0, 1080, 2200)
    private val cssToPhysical: (Float) -> Int = { value -> (value * density).roundToInt() }

    @Test
    fun leftSidePanelWindowExtendsBehindBubble() {
        val bubbleBounds = OverlayBounds(x = 120, y = 360, width = 96, height = 96)

        val spec = panelWindowSpecSupport(
            itemCount = 2,
            bubbleBounds = bubbleBounds,
            density = density,
            screenBounds = screenBounds,
            cssToPhysical = cssToPhysical,
        )

        assertEquals(bubbleBounds.x, spec.x)
        assertTrue(spec.x + spec.width > bubbleBounds.x + bubbleBounds.width)
    }

    @Test
    fun rightSidePanelWindowKeepsOverlapAreaForBubble() {
        val bubbleBounds = OverlayBounds(x = 860, y = 360, width = 96, height = 96)

        val spec = panelWindowSpecSupport(
            itemCount = 2,
            bubbleBounds = bubbleBounds,
            density = density,
            screenBounds = screenBounds,
            cssToPhysical = cssToPhysical,
        )

        assertTrue(spec.x < bubbleBounds.x)
        assertTrue(spec.x + spec.width > bubbleBounds.x)
    }

    @Test
    fun syncPanelWindowStateScriptIncludesBubbleOverlapPadding() {
        val bubbleBounds = OverlayBounds(x = 120, y = 360, width = 96, height = 96)
        val panelBounds = OverlayBounds(x = 120, y = 320, width = 620, height = 320)
        val expectedOverlapCss = panelBubbleOverlapCssWidthSupport(bubbleBounds, density).roundToInt()

        val script = syncPanelWindowStateScriptSupport(
            panelBounds = panelBounds,
            bubbleBounds = bubbleBounds,
            density = density,
            screenBounds = screenBounds,
        )

        assertTrue(script.contains("window.setSide('left', $expectedOverlapCss);"))
    }
}
