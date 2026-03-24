package dev.screengoated.toolbox.mobile.history

import org.junit.Assert.assertEquals
import org.junit.Test

class HistoryModelsTest {
    @Test
    fun clampHistoryLimitUsesWindowsBounds() {
        assertEquals(MIN_HISTORY_LIMIT, clampHistoryLimit(MIN_HISTORY_LIMIT - 9))
        assertEquals(DEFAULT_HISTORY_LIMIT, clampHistoryLimit(DEFAULT_HISTORY_LIMIT))
        assertEquals(MAX_HISTORY_LIMIT, clampHistoryLimit(MAX_HISTORY_LIMIT + 25))
    }

    @Test
    fun normalizeHistorySettingsMigratesLegacyImplicit200Default() {
        val normalized = normalizeHistorySettings(
            HistorySettings(
                maxItems = MAX_HISTORY_LIMIT,
                hasExplicitMaxItems = false,
            ),
        )

        assertEquals(DEFAULT_HISTORY_LIMIT, normalized.maxItems)
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
}
