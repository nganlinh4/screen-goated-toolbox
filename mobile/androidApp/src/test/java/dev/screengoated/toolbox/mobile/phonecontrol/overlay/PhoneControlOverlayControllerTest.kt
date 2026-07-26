package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import kotlinx.coroutines.test.runTest
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
            ),
        )
        assertTrue(needsOverlayLayoutUpdate(true, false))
        assertTrue(needsOverlayLayoutUpdate(false, true))
    }

    @Test
    fun pointerAvoidanceMarksOnlyTheOverlayTransitionScope() = runTest {
        val participant = object : PhoneControlOverlayExclusionParticipant {
            override suspend fun <T> withOverlayAvoiding(
                bounds: OverlayBounds,
                block: suspend () -> T,
            ): T {
                assertTrue(PhoneControlOverlayExclusion.controllerTransitionActive)
                return block()
            }

            override fun interactionBounds() = OverlayBounds(0, 0, 10, 10)
        }
        PhoneControlOverlayExclusion.register(participant)
        try {
            assertFalse(PhoneControlOverlayExclusion.controllerTransitionActive)
            PhoneControlOverlayExclusion.forPoint(5f, 5f) {}
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
    fun captureMaskReadsBoundsWithoutMutatingTheLiveOverlay() = runTest {
        var hidden = false
        var relocated = false
        val participant = object : PhoneControlOverlayExclusionParticipant {
            override suspend fun <T> withOverlayAvoiding(
                bounds: OverlayBounds,
                block: suspend () -> T,
            ): T {
                hidden = true
                relocated = true
                return block()
            }

            override fun interactionBounds() = OverlayBounds(80, 100, 120, 140)

            override fun captureBounds() = OverlayBounds(20, 40, 120, 140)
        }
        PhoneControlOverlayExclusion.register(participant)
        try {
            assertEquals(
                OverlayBounds(20, 40, 120, 140),
                PhoneControlOverlayExclusion.currentCaptureBounds(),
            )
            assertFalse(hidden)
            assertFalse(PhoneControlOverlayExclusion.controllerTransitionActive)
            PhoneControlOverlayExclusion.forPoint(40f, 60f) {}
            assertFalse("Caption pixels must not consume touch", relocated)
        } finally {
            PhoneControlOverlayExclusion.unregister(participant)
        }
    }

    @Test
    fun canonicalRendererRegionScalesFromCssViewportIntoDisplayPixels() {
        assertEquals(
            OverlayBounds(20, 40, 240, 320),
            scaleRendererRegion(
                x = 10.0,
                y = 20.0,
                regionWidth = 110.0,
                regionHeight = 140.0,
                viewportWidth = 540.0,
                viewportHeight = 1_212.0,
                viewWidth = 1_080,
                viewHeight = 2_424,
            ),
        )
    }

    @Test
    fun controllerMaskIsPaddedAndClampedToTheCapturedDisplay() {
        assertEquals(
            OverlayBounds(0, 8, 112, 122),
            controllerOverlayMaskBounds(
                overlay = OverlayBounds(0, 20, 100, 110),
                bitmapWidth = 200,
                bitmapHeight = 130,
            ),
        )
        assertEquals(
            OverlayBounds(187, 87, 200, 100),
            controllerOverlayMaskBounds(
                overlay = OverlayBounds(190, 90, 210, 110),
                bitmapWidth = 200,
                bitmapHeight = 100,
            ),
        )
    }

    @Test
    fun pointerExclusionRelocatesInsteadOfHidingWhenSupported() = runTest {
        var relocated = false
        var hidden = false
        val participant = object : PhoneControlOverlayExclusionParticipant {
            override suspend fun <T> withOverlayAvoiding(
                bounds: OverlayBounds,
                block: suspend () -> T,
            ): T {
                relocated = 5 >= bounds.left && 5 < bounds.right &&
                    5 >= bounds.top && 5 < bounds.bottom
                return block()
            }

            override fun interactionBounds() = OverlayBounds(0, 0, 10, 10)
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
}
