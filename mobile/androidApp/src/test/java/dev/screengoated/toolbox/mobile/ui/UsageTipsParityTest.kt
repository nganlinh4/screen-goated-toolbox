package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
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

class UsageTipsParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun usageTipsUseStaticEntryContract() {
        val windows = fixtureCase("windows_static_entry_contract")
        assertEquals("lightbulb", windows.getValue("icon").jsonPrimitive.content)
        assertFalse(windows.getValue("rotating_preview").jsonPrimitive.boolean)
        assertTrue(windows.getValue("full_list_on_activate").jsonPrimitive.boolean)

        val android = fixtureCase("android_settings_surface")
        assertEquals("static_card", android.getValue("entry_surface").jsonPrimitive.content)
        assertFalse(android.getValue("rotating_preview").jsonPrimitive.boolean)
    }

    @Test
    fun usageTipsExistForAllSupportedLocales() {
        val en = MobileLocaleText.forLanguage("en")
        val vi = MobileLocaleText.forLanguage("vi")
        val ko = MobileLocaleText.forLanguage("ko")

        assertEquals(en.usageTipsList.size, vi.usageTipsList.size)
        assertEquals(en.usageTipsList.size, ko.usageTipsList.size)
        listOf(en, vi, ko).forEach { locale ->
            assertTrue(locale.usageTipsList.isNotEmpty())
            locale.usageTipsList.forEach { tip ->
                assertTrue(tip.isNotBlank())
                assertEquals(
                    "unbalanced bold markers: $tip",
                    0,
                    tip.windowed(2).count { it == "**" } % 2,
                )
            }
        }
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
        assertEquals("static_card", case.getValue("entry_surface").jsonPrimitive.content)
        assertEquals("lightbulb", case.getValue("icon").jsonPrimitive.content)
        assertFalse(case.getValue("rotating_preview").jsonPrimitive.boolean)
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
            "quick settings bubble favorites" -> listOf("quick settings", "bubble", "★")
            "dimmed screen selection cancel" -> listOf("dimmed", "screen", "select", "cancel")
            "history cleanup" -> listOf("history", "oldest")
            "single auto-copy step" -> listOf("one step", "auto-copy")
            "auto-paste accessibility" -> listOf("auto-paste", "accessibility")
            "audio auto-stop" -> listOf("audio", "auto-stop")
            "live translate auto speed" -> listOf("live translate", "auto", "speed")
            "model priority fallback" -> listOf("model priority", "fallback")
            "phone control screen sharing" -> listOf("phone control", "screen sharing")
            "translation gummy" -> listOf("translation gummy")
            "creation tools background jobs" -> listOf("image to 3d", "image to svg", "background")
            "continuous mode" -> listOf("continuous mode")
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
