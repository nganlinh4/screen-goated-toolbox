package dev.screengoated.toolbox.mobile.history

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class HistoryModelsTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun constantsMatchSharedHistoryFixture() {
        val root = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        val range = root.getValue("maxItemsRange").jsonObject

        assertEquals(root.getValue("defaultMaxItems").jsonPrimitive.int, DEFAULT_HISTORY_LIMIT)
        assertEquals(range.getValue("min").jsonPrimitive.int, MIN_HISTORY_LIMIT)
        assertEquals(range.getValue("max").jsonPrimitive.int, MAX_HISTORY_LIMIT)
    }

    @Test
    fun clampHistoryLimitUsesWindowsBounds() {
        assertEquals(MIN_HISTORY_LIMIT, clampHistoryLimit(MIN_HISTORY_LIMIT - 9))
        assertEquals(DEFAULT_HISTORY_LIMIT, clampHistoryLimit(DEFAULT_HISTORY_LIMIT))
        assertEquals(MAX_HISTORY_LIMIT, clampHistoryLimit(MAX_HISTORY_LIMIT + 25))
    }

    @Test
    fun normalizeHistorySettingsMigratesLegacyImplicit200Default() {
        val case = fixtureCase("legacy_android_default_200_migrates_to_windows_default_50")
        val normalized = normalizeHistorySettings(
            HistorySettings(
                maxItems = case.getValue("storedSettings").jsonObject.getValue("maxItems").jsonPrimitive.int,
                hasExplicitMaxItems = false,
            ),
        )

        assertEquals(case.getValue("expectedNormalizedMaxItems").jsonPrimitive.int, normalized.maxItems)
    }

    @Test
    fun normalizeHistorySettingsKeepsExplicit200Selection() {
        val normalized = normalizeHistorySettings(
            HistorySettings(
                maxItems = MAX_HISTORY_LIMIT,
                hasExplicitMaxItems = true,
            ),
        )

        assertEquals(MAX_HISTORY_LIMIT, normalized.maxItems)
    }

    @Test
    fun filterHistoryItemsMatchesTextAndTimestampOnly() {
        val items = listOf(
            HistoryItem(
                id = 1L,
                timestamp = "2026-03-21 10:15:00",
                itemType = HistoryType.TEXT,
                text = "Translated hello world",
                mediaPath = "hello.txt",
            ),
            HistoryItem(
                id = 2L,
                timestamp = "2026-03-22 08:00:00",
                itemType = HistoryType.IMAGE,
                text = "Receipt summary",
                mediaPath = "invoice.png",
            ),
        )

        assertEquals(listOf(items.first()), filterHistoryItems(items, "hello"))
        assertEquals(listOf(items.last()), filterHistoryItems(items, "2026-03-22"))
        assertEquals(emptyList<HistoryItem>(), filterHistoryItems(items, "invoice"))
    }

    @Test
    fun filterHistoryItemsMatchesFixtureTimestampOnlyCase() {
        val case = fixtureCase("search_matches_text_and_timestamp_only")
        val items = case.getValue("items").jsonArray.map { item ->
            val obj = item.jsonObject
            HistoryItem(
                id = obj.getValue("id").jsonPrimitive.content.toLong(),
                timestamp = obj.getValue("timestamp").jsonPrimitive.content,
                itemType = HistoryType.valueOf(obj.getValue("itemType").jsonPrimitive.content),
                text = obj.getValue("text").jsonPrimitive.content,
                mediaPath = obj.getValue("mediaPath").jsonPrimitive.content,
            )
        }

        assertEquals(
            case.getValue("expectedMatchCount").jsonPrimitive.int,
            filterHistoryItems(items, case.getValue("query").jsonPrimitive.content).size,
        )
        assertEquals(emptyList<HistoryItem>(), filterHistoryItems(items, "text_1.txt"))
    }

    private fun fixtureCase(name: String) = json
        .parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
        .jsonObject
        .getValue("cases")
        .jsonArray
        .map { it.jsonObject }
        .firstOrNull { it.getValue("name").jsonPrimitive.content == name }
        ?: error("Missing fixture case: $name")

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "history-ui", "state-machine.json"),
            Paths.get("..", "..", "parity-fixtures", "history-ui", "state-machine.json"),
            Paths.get("parity-fixtures", "history-ui", "state-machine.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing history fixture. Tried: $candidates")
    }
}
