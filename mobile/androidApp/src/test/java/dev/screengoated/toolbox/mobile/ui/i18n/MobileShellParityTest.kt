package dev.screengoated.toolbox.mobile.ui.i18n

import dev.screengoated.toolbox.mobile.branding.MobileBrandAssets
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.next
import dev.screengoated.toolbox.mobile.ui.MobileShellSection
import dev.screengoated.toolbox.mobile.ui.credentialsProviderOrder
import dev.screengoated.toolbox.mobile.ui.layoutBehavior
import dev.screengoated.toolbox.mobile.ui.shouldLockPagerForCarouselTouch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MobileShellParityTest {
    @Test
    fun themeCycleMatchesWindowsTitleBarOrder() {
        assertEquals(MobileThemeMode.DARK, MobileThemeMode.SYSTEM.next())
        assertEquals(MobileThemeMode.LIGHT, MobileThemeMode.DARK.next())
        assertEquals(MobileThemeMode.SYSTEM, MobileThemeMode.LIGHT.next())
    }

    @Test
    fun languageChoicesMatchWindowsVisibleOptions() {
        val options = MobileLocaleText.forLanguage("en").languageOptions.map { it.code }
        assertEquals(listOf("en", "vi", "ko"), options)
    }

    @Test
    fun previewTextComesFromUiLocaleBundle() {
        val en = MobileLocaleText.forLanguage("en").ttsPreviewTexts.first()
        val vi = MobileLocaleText.forLanguage("vi").ttsPreviewTexts.first()
        val ko = MobileLocaleText.forLanguage("ko").ttsPreviewTexts.first()

        assertTrue(en.startsWith("Hello"))
        assertTrue(vi.startsWith("Xin chào"))
        assertTrue(ko.startsWith("안녕하세요"))
    }

    @Test
    fun overlayChromeTextComesFromUiLocaleBundle() {
        val en = MobileLocaleText.forLanguage("en").overlay.placeholderText
        val vi = MobileLocaleText.forLanguage("vi").overlay.placeholderText
        val ko = MobileLocaleText.forLanguage("ko").overlay.placeholderText

        assertTrue(en.startsWith("Waiting"))
        assertTrue(vi.startsWith("Đang chờ"))
        assertTrue(ko.startsWith("음성을"))
    }

    @Test
    fun helpAssistantStringsExistForAllSupportedLocales() {
        val en = MobileLocaleText.forLanguage("en")
        val vi = MobileLocaleText.forLanguage("vi")
        val ko = MobileLocaleText.forLanguage("ko")

        assertEquals("How to use", en.shellHelpLabel)
        assertEquals("Hỏi cách dùng", vi.shellHelpLabel)
        assertEquals("사용법 문의", ko.shellHelpLabel)
        assertTrue(en.helpAssistantAndroidOption.contains("Android"))
        assertTrue(vi.helpAssistantAndroidOption.contains("Android"))
        assertTrue(ko.helpAssistantAndroidOption.contains("Android"))
    }

    @Test
    fun credentialsProviderOrderMatchesWindowsGlobalSettings() {
        assertEquals(
            listOf("Groq", "Cerebras", "Gemini", "OpenRouter", "Ollama"),
            credentialsProviderOrder().map { it.label },
        )
    }

    @Test
    fun windowsBrandIconPairIsTheCanonicalMobileBrandSource() {
        assertEquals("assets/app-icon-small.png", MobileBrandAssets.WINDOWS_DARK_ICON_SOURCE)
        assertEquals("assets/app-icon-small-light.png", MobileBrandAssets.WINDOWS_LIGHT_ICON_SOURCE)
    }

    @Test
    fun toolsTabOwnsItsScrollAndViewportFooter() {
        val behavior = MobileShellSection.TOOLS.layoutBehavior()

        assertEquals(false, behavior.usesOuterScroll)
        assertEquals(true, behavior.usesViewportFooter)
    }

    @Test
    fun nestedCarouselLocksPagerForAnyTouchWhileInnerHorizontalScrollExists() {
        assertTrue(
            shouldLockPagerForCarouselTouch(
                canScrollBackward = true,
                canScrollForward = false,
            ),
        )
        assertTrue(
            shouldLockPagerForCarouselTouch(
                canScrollBackward = false,
                canScrollForward = true,
            ),
        )
        assertTrue(
            shouldLockPagerForCarouselTouch(
                canScrollBackward = true,
                canScrollForward = true,
            ),
        )
        assertFalse(
            shouldLockPagerForCarouselTouch(
                canScrollBackward = false,
                canScrollForward = false,
            ),
        )
    }
}
