package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
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
import org.junit.Assert.assertTrue
import org.junit.Test

class UsageTipsParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun displayDurationMatchesWindowsFormula() {
        val case = fixtureCase("windows_rotation_contract")
        assertEquals(500, case.getValue("fade_duration_ms").jsonPrimitive.int)
        assertEquals("2000 + tip.length * 60", case.getValue("display_duration_formula").jsonPrimitive.content)
        assertEquals(case.getValue("fade_duration_ms").jsonPrimitive.int.toLong(), USAGE_TIP_FADE_DURATION_MS)
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
        repeat(20) { current ->
            val currentIndex = current % 5
            val candidate = selectNextUsageTipIndex(
                currentIndex = currentIndex,
                tipCount = 5,
                random = Random(current),
            )
            assertTrue(candidate in 0..4)
            assertFalse("current=$currentIndex", candidate == currentIndex)
        }
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
        assertEquals(
            -1,
            selectNextUsageTipIndex(
                currentIndex = -1,
                tipCount = 0,
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
        val case = fixtureCase("android_filtered_parity_content")
        val tips = MobileLocaleText.forLanguage("en").usageTipsList.joinToString("\n")

        case.getValue("excluded_desktop_only_concepts").jsonArray
            .map { it.jsonPrimitive.content }
            .forEach { excluded ->
                assertFalse("unexpected desktop-only tip: $excluded", tips.contains(excluded, ignoreCase = true))
            }
        case.getValue("required_concepts").jsonArray
            .map { it.jsonPrimitive.content }
            .forEach { concept ->
                assertTrue("missing usage-tip concept: $concept", containsConcept(tips, concept))
            }
    }

    @Test
    fun usageTipSurfaceMatchesFixture() {
        val case = fixtureCase("android_settings_surface")

        assertEquals("SETTINGS", case.getValue("section").jsonPrimitive.content)
        assertEquals("card", case.getValue("entry_surface").jsonPrimitive.content)
        assertEquals("dialog", case.getValue("full_list_surface").jsonPrimitive.content)
    }

    private fun containsConcept(tips: String, concept: String): Boolean {
        val normalized = tips.lowercase()
        return conceptKeywords(concept).all { word ->
            normalized.contains(word)
        }
    }

    private fun conceptKeywords(concept: String): List<String> {
        return when (concept) {
            "dimmed screen selection cancel" -> listOf("dimmed", "screen", "select", "cancel")
            "history cleanup" -> listOf("history", "clean")
            "single auto-copy step" -> listOf("one step", "auto copy")
            "auto-paste requires caret" -> listOf("auto-paste", "cursor")
            "smart audio stop" -> listOf("audio recording", "smart stop")
            "display mode toggle" -> listOf("display mode", "switch")
            else -> concept.split(' ').map { it.lowercase() }
        }
    }

    private fun fixtureCase(name: String) =
        loadFixture().getValue("cases").jsonArray
            .map { it.jsonObject }
            .first { it.getValue("name").jsonPrimitive.content == name }

    private fun loadFixture() =
        json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/mobile-shell/usage-tips.json"
    }
}
