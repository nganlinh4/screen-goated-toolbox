package dev.screengoated.toolbox.mobile.history

import java.io.File

internal enum class HistoryWriteResult { COMMITTED, REJECTED, UNAVAILABLE }

internal interface HistoryStorage {
    fun paths(): HistoryPaths
    fun loadDatabase(): StoredHistoryDatabase
    fun loadSettings(): HistorySettings
    fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult
    fun saveSettings(settings: HistorySettings): HistoryWriteResult
    fun createMedia(type: HistoryType, bytes: ByteArray): HistoryPreparedMedia
    fun removeMedia(item: HistoryItem)
    fun mediaFile(item: HistoryItem): File?
    fun mediaProofForOpen(item: HistoryItem): HistoryMediaProof?
}
