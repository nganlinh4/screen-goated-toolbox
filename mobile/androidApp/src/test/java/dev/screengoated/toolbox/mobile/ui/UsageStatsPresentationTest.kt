package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.preset.ModelUsageStats
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.preset.PresetProviderSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.long
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UsageStatsPresentationTest {
    @Test
    fun duplicateRolesCollapseToFastestRepresentative() {
        val models = listOf(
            model("demo-text", PresetModelProvider.GROQ, "vendor/model", 900),
            model("demo-vision", PresetModelProvider.GROQ, "vendor/model", 700),
            model("other-text", PresetModelProvider.OPENROUTER, "vendor/model", 600),
            model("local-ollama", PresetModelProvider.OLLAMA, "local/ollama", 100),
        )

        val rows = usageEndpointRepresentatives(models)
        assertEquals(2, rows.size)
        assertTrue(rows.any { it.id == "demo-vision" })
        assertTrue(rows.any { it.id == "other-text" })
        assertFalse(rows.any { it.provider == PresetModelProvider.OLLAMA })
    }

    @Test
    fun freshnessAndLocalesMatchSharedContract() {
        val fixture = Json.parseToJsonElement(
            File(repoRoot(), FIXTURE_PATH).readText(),
        ).jsonObject
        assertEquals(9, fixture.getValue("version").jsonPrimitive.int)
        assertEquals(
            300L,
            fixture.getValue("freshness")
                .jsonObject
                .getValue("fresh_through_seconds")
                .jsonPrimitive
                .long,
        )
        assertEquals(
            900L,
            fixture.getValue("freshness")
                .jsonObject
                .getValue("aging_through_seconds")
                .jsonPrimitive
                .long,
        )
        val presentation = fixture.getValue("presentation").jsonObject
        val quota = presentation.getValue("live_quota").jsonObject
        assertEquals(LOW_QUOTA_RATIO, quota.getValue("warning_at_or_below_ratio").jsonPrimitive.double, 0.0)
        quota.getValue("color_cases").jsonArray.forEach { entry ->
            val case = entry.jsonObject
            val metric = ModelUsageStats.UsageMetric(
                kind = ModelUsageStats.MetricKind.TOKENS_MINUTE,
                remaining = case.getValue("remaining").jsonPrimitive.contentOrNull,
                limit = case.getValue("limit").jsonPrimitive.contentOrNull,
                reset = null,
            )
            val now = 10_000L
            val age = case.getValue("age_seconds").jsonPrimitive.long
            val freshness = ModelUsageStats.freshnessAt(now - age, now)
            val expected = UsageQuotaTone.valueOf(case.getValue("color").jsonPrimitive.content.uppercase())
            assertEquals(expected, usageQuotaTone(metric, freshness))
        }
        assertTrue(
            presentation.getValue("desktop_borderless_table").jsonPrimitive.boolean,
        )
        assertEquals(
            "hidden",
            presentation.getValue("provider_endpoint_count_visibility").jsonPrimitive.content,
        )
        assertEquals(
            "inline",
            presentation.getValue("model_id_placement").jsonPrimitive.content,
        )
        assertEquals(
            1,
            presentation.getValue("endpoint_identity_lines").jsonPrimitive.int,
        )
        val localRuntimeProviders = fixture.getValue("local_runtime_providers")
            .jsonArray
            .map { it.jsonPrimitive.content }
            .toSet()
        assertEquals(setOf("ollama"), localRuntimeProviders)

        listOf("en", "vi", "ko").forEach { language ->
            val locale = MobileLocaleText.forLanguage(language)
            val metric = ModelUsageStats.UsageMetric(ModelUsageStats.MetricKind.TOKENS_MINUTE, "9500", "10000", null)
            val expected = quota.getValue("example_labels").jsonObject.getValue(language).jsonPrimitive.content
            assertEquals(expected, usageQuotaLabel(metric, locale))
            assertTrue(locale.usageStatsSessionHint.isNotBlank())
            listOf("Groq", "OpenRouter").forEach { provider ->
                assertTrue(locale.usageStatsNoData.contains(provider))
            }
            assertTrue(locale.usageStatsCheckUsage.isNotBlank())
        }
    }

    @Test
    fun nvidiaSectionFollowsItsSharedProviderToggle() {
        val hidden = usageProviderSections(PresetProviderSettings(useNvidia = false))
            .single { it.primaryProvider == PresetModelProvider.NVIDIA }
        val visible = usageProviderSections(PresetProviderSettings(useNvidia = true))
            .single { it.primaryProvider == PresetModelProvider.NVIDIA }

        assertFalse(hidden.enabled)
        assertTrue(visible.enabled)
    }

    private fun model(
        id: String,
        provider: PresetModelProvider,
        fullName: String,
        latency: Int,
    ) = PresetModelDescriptor(
        id = id,
        provider = provider,
        fullName = fullName,
        modelType = PresetModelType.TEXT,
        displayName = id,
        typicalLatencyMs = latency,
    )

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/usage-statistics/contract.json"
    }
}
