package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.i18n.MobileUsageTipCategoryId
import dev.screengoated.toolbox.mobile.ui.i18n.MobileUsageTipId
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
    fun usageTipsUseStaticCategorizedSurfaceContract() {
        val windows = fixtureCase("windows_static_entry_contract")
        assertEquals("lightbulb", windows.getValue("icon").jsonPrimitive.content)
        assertFalse(windows.getValue("rotating_preview").jsonPrimitive.boolean)
        assertTrue(windows.getValue("full_list_on_activate").jsonPrimitive.boolean)

        val android = fixtureCase("android_settings_surface")
        assertEquals("SETTINGS", android.getValue("section").jsonPrimitive.content)
        assertEquals("static_card", android.getValue("entry_surface").jsonPrimitive.content)
        assertEquals("lightbulb", android.getValue("icon").jsonPrimitive.content)
        assertFalse(android.getValue("rotating_preview").jsonPrimitive.boolean)
        assertEquals(
            "categorized_dialog",
            android.getValue("full_list_surface").jsonPrimitive.content,
        )
        assertEquals(
            "stacked_sections",
            android.getValue("category_navigation").jsonPrimitive.content,
        )
    }

    @Test
    fun localesKeepCanonicalCategoriesAndStableTipIds() {
        val contract = loadFixture().getValue("catalog_contract").jsonObject
        assertFalse(contract.getValue("ordinal_markers").jsonPrimitive.boolean)
        assertTrue(contract.getValue("stable_tip_ids").jsonPrimitive.boolean)
        assertEquals("omit", contract.getValue("empty_categories").jsonPrimitive.content)

        val fixtureCategories = contract.getValue("categories").jsonArray.map { it.jsonObject }
        val categoryIds = fixtureCategories.map { it.getValue("id").jsonPrimitive.content }
        assertEquals(
            categoryIds,
            MobileUsageTipCategoryId.values().map { it.stableId },
        )

        val locales = listOf(
            MobileLocaleText.forLanguage("en"),
            MobileLocaleText.forLanguage("vi"),
            MobileLocaleText.forLanguage("ko"),
        )
        val expectedTipIds = requiredAndroidTipIds()

        locales.forEach { locale ->
            val categories = locale.usageTipsCategories.filter { it.tips.isNotEmpty() }
            assertEquals(categoryIds, categories.map { it.id.stableId })
            assertTrue(locale.usageTipsTitle.isNotBlank())
            assertTrue(locale.usageTipsClickHint.isNotBlank())
            assertTrue(locale.usageTipsDescription.isNotBlank())

            categories.zip(fixtureCategories).forEach { (category, fixtureCategory) ->
                val expectedLabel = fixtureCategory
                    .getValue("labels")
                    .jsonObject
                    .getValue(locale.localeCode)
                    .jsonPrimitive
                    .content
                assertEquals(expectedLabel, category.title)
                assertTrue(category.description.isNotBlank())
                assertTrue(category.tips.isNotEmpty())
            }

            val tips = categories.flatMap { it.tips }
            val tipIds = tips.map { it.id.stableId }
            assertEquals(expectedTipIds, tipIds)
            assertEquals(tipIds.size, tipIds.toSet().size)
            tips.forEach { tip ->
                assertTrue(tip.text.isNotBlank())
                assertEquals(
                    "unbalanced bold markers: ${tip.id.stableId}",
                    0,
                    tip.text.windowed(2).count { it == "**" } % 2,
                )
            }
        }

        assertEquals(
            expectedTipIds,
            MobileUsageTipId.values().map { it.stableId },
        )
    }

    @Test
    fun androidCatalogIncludesRequiredIdsAndExcludesDesktopOnlyIds() {
        val case = fixtureCase("android_filtered_parity_content")
        val tipIds = MobileLocaleText.forLanguage("en")
            .usageTipsCategories
            .flatMap { it.tips }
            .map { it.id.stableId }

        assertEquals(requiredAndroidTipIds(), tipIds)
        case.getValue("excluded_tip_ids").jsonArray
            .map { it.jsonPrimitive.content }
            .forEach { excluded ->
                assertFalse("unexpected Android usage-tip id: $excluded", excluded in tipIds)
            }
    }

    @Test
    fun categorizedDialogHasNoOrdinalTipBadges() {
        val contract = loadFixture().getValue("catalog_contract").jsonObject
        assertFalse(contract.getValue("ordinal_markers").jsonPrimitive.boolean)

        val source = File(repoRoot(), UI_SOURCE_PATH).readText()
        assertFalse(source.contains("tipNumber"))
        assertFalse(source.contains("forEachIndexed"))
    }

    private fun requiredAndroidTipIds(): List<String> =
        fixtureCase("android_filtered_parity_content")
            .getValue("required_tip_ids")
            .jsonArray
            .map { it.jsonPrimitive.content }

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
        private const val UI_SOURCE_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/UsageTipsUi.kt"
    }
}
