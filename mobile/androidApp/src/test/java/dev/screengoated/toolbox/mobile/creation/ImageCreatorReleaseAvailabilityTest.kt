package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.releasedAppSlotIndices
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ImageCreatorReleaseAvailabilityTest {
    @Test
    fun releaseGateUsesSharedDisabledFlagAndLocalizedCopy() {
        assertFalse(BuildConfig.IMAGE_CREATOR_RELEASE_ENABLED)
        assertFalse(BuildConfig.IMAGE_TO_SVG_RELEASE_ENABLED)
        assertFalse(creationToolReleased(CreationTool.IMAGE_CREATOR))
        assertFalse(creationToolReleased(CreationTool.IMAGE_TO_SVG))
        assertEquals((0..5).toList(), releasedAppSlotIndices())
        assertEquals("Coming soon", MobileLocaleText.forLanguage("en").comingSoonLabel)
        assertEquals("곧 출시", MobileLocaleText.forLanguage("ko").comingSoonLabel)
        assertEquals("Sắp ra mắt", MobileLocaleText.forLanguage("vi").comingSoonLabel)
    }
}
