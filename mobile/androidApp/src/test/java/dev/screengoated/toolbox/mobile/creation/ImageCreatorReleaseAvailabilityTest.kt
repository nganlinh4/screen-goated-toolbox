package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.ui.releasedAppSlotIndices
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ImageCreatorReleaseAvailabilityTest {
    @Test
    fun releaseGateHidesImageCreatorAndKeepsSvgLauncher() {
        assertFalse(BuildConfig.IMAGE_CREATOR_RELEASE_ENABLED)
        assertTrue(BuildConfig.IMAGE_TO_SVG_RELEASE_ENABLED)
        assertFalse(creationToolReleased(CreationTool.IMAGE_CREATOR))
        assertTrue(creationToolReleased(CreationTool.IMAGE_TO_SVG))
        assertEquals((0..6).toList(), releasedAppSlotIndices())
    }
}
