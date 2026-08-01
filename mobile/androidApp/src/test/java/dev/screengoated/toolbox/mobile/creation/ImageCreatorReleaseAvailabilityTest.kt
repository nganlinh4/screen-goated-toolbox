package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ImageCreatorReleaseAvailabilityTest {
    @Test
    fun releaseGateUsesSharedDisabledFlagAndLocalizedCopy() {
        assertFalse(BuildConfig.IMAGE_CREATOR_RELEASE_ENABLED)
        assertFalse(creationToolReleased(CreationTool.IMAGE_CREATOR))
        assertEquals("Coming soon", MobileLocaleText.forLanguage("en").comingSoonLabel)
        assertEquals("곧 출시", MobileLocaleText.forLanguage("ko").comingSoonLabel)
        assertEquals("Sắp ra mắt", MobileLocaleText.forLanguage("vi").comingSoonLabel)
    }
}
