package dev.screengoated.toolbox.mobile.ui.i18n

import dev.screengoated.toolbox.mobile.branding.MobileBrandAssets
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.next
import dev.screengoated.toolbox.mobile.ui.MobileShellSection
import dev.screengoated.toolbox.mobile.ui.compactMethodLabel
import dev.screengoated.toolbox.mobile.ui.credentialsProviderOrder
import dev.screengoated.toolbox.mobile.ui.layoutBehavior
import dev.screengoated.toolbox.mobile.ui.methodLabel
import dev.screengoated.toolbox.mobile.ui.shouldLockPagerForCarouselTouch
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
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
        val options = MobileLocaleText.forLanguage("en").appearance.languageOptions.map { it.code }
        val expected = case.getValue("expected_language_codes").jsonArray.map { it.jsonPrimitive.content }
        assertEquals(expected, options)
    }

    @Test
    fun localeResolutionUsesExplicitCodeAndFallsBackToEnglish() {
        val case = mobileShellFixtureCase("locale_resolution_uses_explicit_code_and_falls_back_to_english")
        case.getValue("cases").jsonArray.forEach { resolutionCase ->
            val resolution = resolutionCase.jsonObject
            val locale = MobileLocaleText.forLanguage(resolution.getValue("input").jsonPrimitive.content)
            val expected = resolution.getValue("expected_locale_code").jsonPrimitive.content

            assertEquals(expected, locale.localeCode)
            assertEquals(expected, locale.languageCode())
        }
    }

    @Test
    fun previewTextComesFromUiLocaleBundle() {
        val case = mobileShellFixtureCase("localized_preview_text_comes_from_ui_language_bundle")
        val voiceName = case.getValue("voice_name").jsonPrimitive.content
        val expectedTemplateCount = case.getValue("expected_template_count").jsonPrimitive.int
        val avoidImmediateRepeat = case.getValue("avoid_immediate_repeat_when_multiple").jsonPrimitive.boolean
        case.getValue("locales").jsonArray.forEach { localeCase ->
            val localeObject = localeCase.jsonObject
            val locale = MobileLocaleText.forLanguage(
                localeObject.getValue("ui_language").jsonPrimitive.content,
            )
            val templates = locale.ttsVoice.previewTexts
            val firstTemplate = templates.first()

            assertEquals(expectedTemplateCount, templates.size)
            assertTrue(firstTemplate.startsWith(localeObject.getValue("expected_prefix").jsonPrimitive.content))
            assertEquals(
                localeObject.getValue("expected_first_rendered").jsonPrimitive.content,
                firstTemplate.replace("{}", voiceName),
            )

            if (avoidImmediateRepeat && templates.size > 1) {
                val seed = 0
                val firstCandidate = Random(seed).nextInt(templates.size)
                val selection = locale.nextPreviewText(
                    voiceName = voiceName,
                    previousIndex = firstCandidate,
                    random = Random(seed),
                )
                assertNotEquals(firstCandidate, selection.index)
            }
        }
    }

    @Test
    fun localeHelpersFollowCopiedSectionsWithoutChangingLocaleIdentity() {
        val case = mobileShellFixtureCase("locale_helpers_follow_copied_sections")
        val base = MobileLocaleText.forLanguage(case.getValue("base_locale").jsonPrimitive.content)
        val copied = MobileLocaleText.forLanguage(case.getValue("copied_section_locale").jsonPrimitive.content)
        val copiedSections = case.getValue("copied_sections").jsonArray
            .map { it.jsonPrimitive.content }
        val hybrid = copiedSections.fold(base) { locale, section ->
            when (section) {
                "appearance" -> locale.copy(appearance = copied.appearance)
                "help" -> locale.copy(help = copied.help)
                "ttsVoice" -> locale.copy(ttsVoice = copied.ttsVoice)
                else -> error("Unknown copied locale section: $section")
            }
        }

        val expectedLocaleCode = case.getValue("expected_locale_code_after_copy").jsonPrimitive.content
        assertEquals(expectedLocaleCode, hybrid.localeCode)
        assertEquals(expectedLocaleCode, hybrid.languageCode())

        case.getValue("helpers").jsonArray.map { it.jsonPrimitive.content }.forEach { helper ->
            when (helper) {
                "compact_overlay_opacity" -> assertEquals(
                    copied.compactOverlayOpacityLabel(),
                    hybrid.compactOverlayOpacityLabel(),
                )
                "reset_defaults_done" -> assertEquals(
                    copied.resetDefaultsDoneMessage(),
                    hybrid.resetDefaultsDoneMessage(),
                )
                "tools_help" -> assertEquals(copied.toolsHelpCopy(), hybrid.toolsHelpCopy())
                "preview_selection" -> assertEquals(
                    copied.nextPreviewText("Aoede", previousIndex = 0, random = Random(7)),
                    hybrid.nextPreviewText("Aoede", previousIndex = 0, random = Random(7)),
                )
                else -> error("Unknown locale helper contract: $helper")
            }
        }
    }

    @Test
    fun settingsActionFeedbackComesFromUiLocaleBundle() {
        val case = mobileShellFixtureCase("settings_action_feedback_comes_from_ui_language_bundle")
        case.getValue("locales").jsonArray.forEach { localeCase ->
            val localeObject = localeCase.jsonObject
            val locale = MobileLocaleText.forLanguage(
                localeObject.getValue("ui_language").jsonPrimitive.content,
            )

            assertEquals(
                localeObject.getValue("expected_compact_overlay_opacity").jsonPrimitive.content,
                locale.compactOverlayOpacityLabel(),
            )
            assertEquals(
                localeObject.getValue("expected_reset_done").jsonPrimitive.content,
                locale.resetDefaultsDoneMessage(),
            )
        }
    }

    @Test
    fun ttsMethodLabelsComeFromUiLocaleBundle() {
        val case = mobileShellFixtureCase("tts_method_labels_come_from_ui_language_bundle")
        case.getValue("locales").jsonArray.forEach { localeCase ->
            val localeObject = localeCase.jsonObject
            val locale = MobileLocaleText.forLanguage(
                localeObject.getValue("ui_language").jsonPrimitive.content,
            )
            val method = MobileTtsMethod.valueOf(localeObject.getValue("method").jsonPrimitive.content)

            assertEquals(
                localeObject.getValue("expected_label").jsonPrimitive.content,
                methodLabel(locale, method),
            )
            assertEquals(
                localeObject.getValue("expected_compact").jsonPrimitive.content,
                compactMethodLabel(locale, method),
            )
        }
    }

    @Test
    fun toolsHelpCopyComesFromUiLocaleBundle() {
        val case = mobileShellFixtureCase("tools_help_copy_comes_from_ui_language_bundle")
        case.getValue("locales").jsonArray.forEach { localeCase ->
            val localeObject = localeCase.jsonObject
            val locale = MobileLocaleText.forLanguage(
                localeObject.getValue("ui_language").jsonPrimitive.content,
            )
            val copy = locale.toolsHelpCopy()

            assertEquals(
                localeObject.getValue("expected_language_code").jsonPrimitive.content,
                locale.languageCode(),
            )
            assertEquals(
                localeObject.getValue("expected_title").jsonPrimitive.content,
                copy.title,
            )
            assertEquals(
                localeObject.getValue("expected_bubble_title").jsonPrimitive.content,
                copy.bubbleTitle,
            )
            assertEquals(
                localeObject.getValue("expected_dismiss").jsonPrimitive.content,
                copy.dismiss,
            )
        }
    }

    @Test
    fun overlayChromeTextComesFromUiLocaleBundle() {
        val en = MobileLocaleText.forLanguage("en").appearance.overlay.placeholderText
        val vi = MobileLocaleText.forLanguage("vi").appearance.overlay.placeholderText
        val ko = MobileLocaleText.forLanguage("ko").appearance.overlay.placeholderText

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

        assertEquals(
            File(repoRoot(), MobileBrandAssets.WINDOWS_DARK_ICON_SOURCE).readBytes().toList(),
            File(repoRoot(), "mobile/androidApp/src/main/res/drawable-nodpi/sgt_brand_dark.png").readBytes().toList(),
        )
        assertEquals(
            File(repoRoot(), MobileBrandAssets.WINDOWS_LIGHT_ICON_SOURCE).readBytes().toList(),
            File(repoRoot(), "mobile/androidApp/src/main/res/drawable-nodpi/sgt_brand_light.png").readBytes().toList(),
        )
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
