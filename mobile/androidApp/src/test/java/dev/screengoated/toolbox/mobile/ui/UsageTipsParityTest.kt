package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UsageTipsParityTest {
    @Test
    fun displayDurationMatchesWindowsFormula() {
        assertEquals(2000L, usageTipDisplayDurationMillis(""))
        assertEquals(2240L, usageTipDisplayDurationMillis("1234"))
    }

    @Test
    fun nextTipDoesNotRepeatWhenMultipleTipsExist() {
        val next = selectNextUsageTipIndex(
            currentIndex = 1,
            tipCount = 3,
            random = Random(0),
        )

        assertTrue(next in 0..2)
        assertFalse(next == 1)
    }

    @Test
    fun singleTipListStaysOnSameIndex() {
        assertEquals(
            0,
            selectNextUsageTipIndex(
                currentIndex = 0,
                tipCount = 1,
                random = Random(0),
            ),
        )
    }

    @Test
    fun usageTipsExistForAllSupportedLocales() {
        val en = MobileLocaleText.forLanguage("en")
        val vi = MobileLocaleText.forLanguage("vi")
        val ko = MobileLocaleText.forLanguage("ko")

        assertEquals(en.usageTipsList.size, vi.usageTipsList.size)
        assertEquals(en.usageTipsList.size, ko.usageTipsList.size)
        assertTrue(en.usageTipsTitle.isNotBlank())
        assertTrue(vi.usageTipsTitle.isNotBlank())
        assertTrue(ko.usageTipsTitle.isNotBlank())
    }

    @Test
    fun englishTipsFilterDesktopOnlyConcepts() {
        val tips = MobileLocaleText.forLanguage("en").usageTipsList.joinToString("\n")

        assertFalse(tips.contains("Middle-click"))
        assertFalse(tips.contains("Right-click"))
        assertFalse(tips.contains("system tray"))
        assertFalse(tips.contains("drag and drop"))
        assertTrue(tips.contains("**Continuous Mode**"))
        assertTrue(tips.contains("**Auto-paste**"))
        assertTrue(tips.contains("**Audio recording**"))
    }
}
