package dev.screengoated.toolbox.mobile.history

import kotlinx.serialization.Serializable

internal const val DEFAULT_HISTORY_LIMIT: Int = 50
internal const val MIN_HISTORY_LIMIT: Int = 10
internal const val MAX_HISTORY_LIMIT: Int = 200

@Serializable
enum class HistoryType {
    IMAGE,
    AUDIO,
    TEXT,
}

@Serializable
data class HistoryItem(
    val id: Long,
    val timestamp: String,
    val itemType: HistoryType,
    val text: String,
    val mediaPath: String,
)

@Serializable
data class StoredHistoryDatabase(
    val version: Int = 1,
    val items: List<HistoryItem> = emptyList(),
)

@Serializable
data class HistorySettings(
    val maxItems: Int = DEFAULT_HISTORY_LIMIT,
)

data class HistoryUiState(
    val items: List<HistoryItem> = emptyList(),
    val maxItems: Int = DEFAULT_HISTORY_LIMIT,
    val mediaDirectoryPath: String? = null,
    val supportsFolderOpen: Boolean = false,
)

internal fun clampHistoryLimit(value: Int): Int {
    return value.coerceIn(MIN_HISTORY_LIMIT, MAX_HISTORY_LIMIT)
}

internal fun filterHistoryItems(
    items: List<HistoryItem>,
    query: String,
): List<HistoryItem> {
    val normalized = query.trim().lowercase()
    if (normalized.isEmpty()) {
        return items
    }
    return items.filter { item ->
        item.text.lowercase().contains(normalized) || item.timestamp.lowercase().contains(normalized)
    }
}
