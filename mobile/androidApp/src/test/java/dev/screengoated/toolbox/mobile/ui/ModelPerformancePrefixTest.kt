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
    fun catalogDisplayOrderSortsGloballyByLatency() {
        val models = PresetModelCatalog.models
        assertEquals(
            models.map { it.typicalLatencyMs ?: Int.MAX_VALUE }.sorted(),
            models.map { it.typicalLatencyMs ?: Int.MAX_VALUE },
        )
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
