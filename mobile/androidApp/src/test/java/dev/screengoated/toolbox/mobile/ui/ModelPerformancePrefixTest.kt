package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ModelPerformancePrefixTest {
    @Test
    fun latencyFormattingMatchesSharedCatalogContract() {
        val cases = Json.parseToJsonElement(
            Files.readAllBytes(fixturePath()).decodeToString(),
        ).jsonObject
            .getValue("performance").jsonObject
            .getValue("latency_format_cases").jsonArray
        cases.forEach { value ->
            val case = value.jsonObject
            assertEquals(
                case.getValue("label").jsonPrimitive.content,
                formatModelLatencyMs(case.getValue("milliseconds").jsonPrimitive.int),
            )
        }
        assertEquals("—", formatModelLatencyMs(null))
    }

    @Test
    fun sixIntelligenceLevelsUseTheSharedStatScale() {
        val expectedNames = fixture()
            .getValue("performance").jsonObject
            .getValue("intelligence_stat_icons").jsonArray
            .map { it.jsonPrimitive.content }
        assertEquals(
            expectedNames,
            (1..6).map(::intelligenceStatIconName),
        )
        assertEquals(6, (1..6).map(::intelligenceIconResource).distinct().size)
    }

    @Test
    fun compactPrefixColumnsMatchSharedCatalogContract() {
        val columns = fixture().getValue("performance_columns").jsonObject
        assertEquals(
            columns.getValue("intelligence_width").jsonPrimitive.int,
            MODEL_INTELLIGENCE_COLUMN_WIDTH_DP,
        )
        assertEquals(
            columns.getValue("inter_column_gap").jsonPrimitive.int,
            MODEL_PERFORMANCE_COLUMN_GAP_DP,
        )
        assertEquals(
            columns.getValue("latency_width").jsonPrimitive.int,
            MODEL_LATENCY_COLUMN_WIDTH_DP,
        )
    }

    @Test
    fun priorityRowsKeepThePerformancePrefixVisible() {
        val visibility = fixture().getValue("performance_prefix_visibility").jsonObject
        assertEquals(
            true,
            visibility.getValue("priority_chain_rows").jsonPrimitive.content.toBoolean(),
        )
    }

    @Test
    fun livePriorityLatencyExplainsTheAdaptiveRanking() {
        val model = requireNotNull(
            PresetModelCatalog.getById("nvidia-nemotron-3-super-120b-text"),
        )
        assertEquals(483, displayedModelLatencyMs(model, 483))
        assertEquals(model.typicalLatencyMs, displayedModelLatencyMs(model, null))
    }

    @Test
    fun catalogDisplayOrderSortsGloballyByLatency() {
        val models = PresetModelCatalog.models
        assertEquals(
            models.map { it.typicalLatencyMs ?: Int.MAX_VALUE }.sorted(),
            models.map { it.typicalLatencyMs ?: Int.MAX_VALUE },
        )
    }

    @Test
    fun adaptivePriorityRecordsTheLiveFeedPlatformBoundary() {
        val adaptive = fixture().getValue("adaptive_priority").jsonObject
        assertTrue(adaptive.getValue("windows_live_feed").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("android_live_feed").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("live_rows_editable").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("live_row_reorder_creates_pin").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("live_row_delete_creates_exclusion").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("non_live_edits_preserve_enabled").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("row_overrides_preserve_enabled").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("manual_edit_without_live_rows_disables_live").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("dedicated_capabilities_excluded_from_generic_chains").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("reset_clears_row_overrides").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("refresh_reorders_only_while_enabled").jsonPrimitive.content.toBoolean())
        assertEquals(5, adaptive.getValue("maximum_offers_per_chain").jsonPrimitive.int)
        assertEquals(3, adaptive.getValue("minimum_unpinned_live_position").jsonPrimitive.int)
        assertTrue(adaptive.getValue("live_rows_show_ranking_latency").jsonPrimitive.content.toBoolean())
        assertTrue(adaptive.getValue("publisher_owns_offer_admission").jsonPrimitive.content.toBoolean())
        assertTrue(
            adaptive.getValue("feed_absence_removes_nvidia_from_live_routing")
                .jsonPrimitive.content.toBoolean(),
        )
        assertTrue(
            adaptive.getValue("signed_feed_projects_all_nvidia_selectors")
                .jsonPrimitive.content.toBoolean(),
        )
        assertTrue(adaptive.getValue("reviewed_withdrawal_remains_quality_veto").jsonPrimitive.content.toBoolean())
        assertEquals(3, adaptive.getValue("signed_feed_schema").jsonPrimitive.int)
        assertEquals(1, adaptive.getValue("availability_gate_version").jsonPrimitive.int)
        assertEquals(
            "atomic_with_same_directory_fallback",
            adaptive.getValue("verified_cache_replace").jsonPrimitive.content,
        )
    }

    @Test
    fun priorityChainTargetsDoNotLimitUserRows() {
        val size = fixture().getValue("priority_chain_size").jsonObject
        assertTrue(size.getValue("user_limit").toString() == "null")
        assertEquals(10, size.getValue("prepared_image_default_target").jsonPrimitive.int)
        assertEquals(12, size.getValue("prepared_text_default_target").jsonPrimitive.int)
    }

    @Test
    fun priorityNumberingKeepsSentinelsOutsideTheEditableSequence() {
        val numbering = fixture().getValue("priority_numbering").jsonObject
        assertEquals(0, numbering.getValue("chosen_model").jsonPrimitive.int)
        assertEquals(1, numbering.getValue("first_editable_model").jsonPrimitive.int)
        assertEquals("next", numbering.getValue("automatic_fallback").jsonPrimitive.content)
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "model-catalog", "presentation.json"),
            Paths.get("..", "..", "parity-fixtures", "model-catalog", "presentation.json"),
            Paths.get("parity-fixtures", "model-catalog", "presentation.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Missing model catalog presentation fixture. Tried: $candidates")
    }

    private fun fixture() = Json.parseToJsonElement(
        Files.readAllBytes(fixturePath()).decodeToString(),
    ).jsonObject
}
