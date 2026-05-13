package dev.screengoated.toolbox.mobile.ui.i18n

import dev.screengoated.toolbox.mobile.branding.MobileBrandAssets
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.next
import dev.screengoated.toolbox.mobile.ui.MobileShellSection
import dev.screengoated.toolbox.mobile.ui.credentialsProviderOrder
import dev.screengoated.toolbox.mobile.ui.layoutBehavior
import dev.screengoated.toolbox.mobile.ui.shouldLockPagerForCarouselTouch
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MobileShellParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun themeCycleMatchesWindowsTitleBarOrder() {
        val case = mobileShellFixtureCase("theme_cycle_matches_windows_title_bar")
        val cycle = case.getValue("expected_cycle").jsonArray.map { it.jsonPrimitive.content }
        var mode = MobileThemeMode.valueOf(case.getValue("initial_theme_mode").jsonPrimitive.content)

        cycle.forEach { expected ->
            mode = mode.next()
            assertEquals(MobileThemeMode.valueOf(expected), mode)
        }
    }

    @Test
    fun languageChoicesMatchWindowsVisibleOptions() {
        val case = mobileShellFixtureCase("ui_language_choices_match_windows_title_bar")
        val options = MobileLocaleText.forLanguage("en").languageOptions.map { it.code }
        val expected = case.getValue("expected_language_codes").jsonArray.map { it.jsonPrimitive.content }
        assertEquals(expected, options)
    }

    @Test
    fun previewTextComesFromUiLocaleBundle() {
        val case = mobileShellFixtureCase("localized_preview_text_comes_from_ui_language_bundle")
        case.getValue("locales").jsonArray.forEach { localeCase ->
            val localeObject = localeCase.jsonObject
            val text = MobileLocaleText
                .forLanguage(localeObject.getValue("ui_language").jsonPrimitive.content)
                .ttsPreviewTexts
                .first()
            assertTrue(text.startsWith(localeObject.getValue("expected_prefix").jsonPrimitive.content))
        }
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
        assertEquals("Ask quick", en.helpAssistantQuickOption)
        assertEquals("Hỏi nhanh", vi.helpAssistantQuickOption)
        assertEquals("자세히 묻기", ko.helpAssistantDetailedOption)
        assertTrue(en.helpAssistantAndroidOption.contains("Android"))
        assertTrue(vi.helpAssistantAndroidOption.contains("Android"))
        assertTrue(ko.helpAssistantAndroidOption.contains("Android"))
    }

    @Test
    fun credentialsProviderOrderMatchesWindowsGlobalSettings() {
        val case = credentialsFixtureCase("windows_global_settings_credentials_provider_order")
        assertEquals(
            case.getValue("order").jsonArray.map { it.jsonPrimitive.content },
            credentialsProviderOrder().map { it.label },
        )
    }

    @Test
    fun windowsBrandIconPairIsTheCanonicalMobileBrandSource() {
        val case = mobileShellFixtureCase("mobile_branding_uses_windows_app_icon_pair")
        assertEquals(case.getValue("expected_dark_asset").jsonPrimitive.content, MobileBrandAssets.WINDOWS_DARK_ICON_SOURCE)
        assertEquals(case.getValue("expected_light_asset").jsonPrimitive.content, MobileBrandAssets.WINDOWS_LIGHT_ICON_SOURCE)
    }

    @Test
    fun toolsTabOwnsItsScrollAndViewportFooter() {
        val case = mobileShellFixtureCase("tools_tab_owns_scroll_and_uses_viewport_footer")
        val behavior = MobileShellSection.TOOLS.layoutBehavior()

        assertEquals(case.getValue("expected_outer_scroll_owner").jsonPrimitive.boolean, behavior.usesOuterScroll)
        assertEquals(case.getValue("expected_viewport_footer").jsonPrimitive.boolean, behavior.usesViewportFooter)
    }

    @Test
    fun nestedCarouselLocksPagerForAnyTouchWhileInnerHorizontalScrollExists() {
        val case = mobileShellFixtureCase("nested_carousel_locks_outer_pager_for_the_full_touch_when_inner_scroll_exists")
        case.getValue("touch_cases").jsonArray.forEach { element ->
            val touchCase = element.jsonObject
            assertEquals(
                touchCase.getValue("expected_lock").jsonPrimitive.boolean,
                shouldLockPagerForCarouselTouch(
                    canScrollBackward = touchCase.getValue("can_scroll_backward").jsonPrimitive.boolean,
                    canScrollForward = touchCase.getValue("can_scroll_forward").jsonPrimitive.boolean,
                ),
            )
        }
    }

    private fun mobileShellFixtureCase(name: String) =
        fixtureCase("parity-fixtures/mobile-shell/ui-language-theme.json", name)

    private fun credentialsFixtureCase(name: String) =
        fixtureCase("parity-fixtures/mobile-shell/credentials-provider-order.json", name)

    private fun fixtureCase(path: String, name: String) =
        loadFixture(path).getValue("cases").jsonArray
            .map { it.jsonObject }
            .first { it.getValue("name").jsonPrimitive.content == name }

    private fun loadFixture(path: String) =
        json.parseToJsonElement(File(repoRoot(), path).readText()).jsonObject

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, "parity-fixtures/mobile-shell/ui-language-theme.json").exists()
        } ?: error("Could not locate repo root from $workingDirectory")
    }
}
