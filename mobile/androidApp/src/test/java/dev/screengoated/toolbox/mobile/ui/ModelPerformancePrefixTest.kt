package dev.screengoated.toolbox.mobile.ui

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

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "model-catalog", "presentation.json"),
            Paths.get("..", "..", "parity-fixtures", "model-catalog", "presentation.json"),
            Paths.get("parity-fixtures", "model-catalog", "presentation.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Missing model catalog presentation fixture. Tried: $candidates")
    }
}
