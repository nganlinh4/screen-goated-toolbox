package dev.screengoated.toolbox.mobile.history

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.io.File
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import java.util.concurrent.atomic.AtomicLong

class HistoryRepository internal constructor(
    private val persistence: HistoryPersistence,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private val scope = CoroutineScope(SupervisorJob() + ioDispatcher)
    private val mutex = Mutex()
    private val timestampFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss")
    private val filenameFormatter = DateTimeFormatter.ofPattern("yyyyMMdd_HHmmss_SSS")
    private val nextId = AtomicLong(System.currentTimeMillis())

    private val _state = MutableStateFlow(loadInitialState())
    val state: StateFlow<HistoryUiState> = _state.asStateFlow()

    fun saveText(
        resultText: String,
        inputText: String,
    ) {
        if (resultText.isBlank()) {
            return
        }
        scope.launch {
            mutex.withLock {
                val now = LocalDateTime.now()
                val fileName = "text_${filenameFormatter.format(now)}.txt"
                val file = persistence.mediaFile(fileName)
                runCatching {
                    file.parentFile?.mkdirs()
                    file.writeText(inputText)
                }.onSuccess {
                    upsertItem(
                        HistoryItem(
                            id = nextId.incrementAndGet(),
                            timestamp = timestampFormatter.format(now),
                            itemType = HistoryType.TEXT,
                            text = resultText,
                            mediaPath = fileName,
                        ),
                    )
                }
            }
        }
    }

    fun saveImage(
        pngBytes: ByteArray,
        resultText: String,
    ) {
        if (resultText.isBlank() || pngBytes.isEmpty()) {
            return
        }
        scope.launch {
            mutex.withLock {
                val now = LocalDateTime.now()
                val fileName = "img_${filenameFormatter.format(now)}.png"
                val file = persistence.mediaFile(fileName)
                runCatching {
                    file.parentFile?.mkdirs()
                    file.writeBytes(pngBytes)
                }.onSuccess {
                    upsertItem(
                        HistoryItem(
                            id = nextId.incrementAndGet(),
                            timestamp = timestampFormatter.format(now),
                            itemType = HistoryType.IMAGE,
                            text = resultText,
                            mediaPath = fileName,
                        ),
                    )
                }
            }
        }
    }

    fun saveAudio(
        wavBytes: ByteArray,
        resultText: String,
    ) {
        if (resultText.isBlank() || wavBytes.isEmpty()) {
            return
        }
        scope.launch {
            mutex.withLock {
                val now = LocalDateTime.now()
                val fileName = "audio_${filenameFormatter.format(now)}.wav"
                val file = persistence.mediaFile(fileName)
                runCatching {
                    file.parentFile?.mkdirs()
                    file.writeBytes(wavBytes)
                }.onSuccess {
                    upsertItem(
                        HistoryItem(
                            id = nextId.incrementAndGet(),
                            timestamp = timestampFormatter.format(now),
                            itemType = HistoryType.AUDIO,
                            text = resultText,
                            mediaPath = fileName,
                        ),
                    )
                }
            }
        }
    }

    fun delete(id: Long) {
        scope.launch {
            mutex.withLock {
                val current = _state.value.items.toMutableList()
                val removed = current.firstOrNull { it.id == id } ?: return@withLock
                current.removeAll { it.id == id }
                deleteBackingFile(removed)
                persistItems(current)
            }
        }
    }

    fun clearAll() {
        scope.launch {
            mutex.withLock {
                _state.value.items.forEach(::deleteBackingFile)
                persistItems(emptyList())
            }
        }
    }

    fun updateMaxItems(value: Int) {
        val clamped = clampHistoryLimit(value)
        scope.launch {
            mutex.withLock {
                val current = _state.value
                if (current.maxItems == clamped) {
                    return@withLock
                }
                val settings = HistorySettings(
                    maxItems = clamped,
                    hasExplicitMaxItems = true,
                )
                persistence.saveSettings(settings)
                val prunedItems = pruneItems(current.items, clamped)
                persistItems(prunedItems, maxItems = clamped)
            }
        }
    }

    fun resetSettingsToDefaults() {
        scope.launch {
            mutex.withLock {
                val settings = HistorySettings()
                persistence.saveSettings(settings)
                val prunedItems = pruneItems(_state.value.items, settings.maxItems)
                persistItems(prunedItems, maxItems = settings.maxItems)
            }
        }
    }

    fun mediaFileFor(item: HistoryItem): File? {
        if (item.mediaPath.isBlank()) {
            return null
        }
        return persistence.mediaFile(item.mediaPath)
    }

    fun mediaDirectory(): File = persistence.paths().mediaDir

    private fun loadInitialState(): HistoryUiState {
        val database = persistence.loadDatabase()
        val settings = persistence.loadSettings()
        return HistoryUiState(
            items = pruneItems(database.items, clampHistoryLimit(settings.maxItems), deletePrunedFiles = false),
            maxItems = clampHistoryLimit(settings.maxItems),
            mediaDirectoryPath = persistence.paths().mediaDir.absolutePath,
            supportsFolderOpen = persistence.paths().supportsFolderOpen,
        )
    }

    private fun upsertItem(item: HistoryItem) {
        val nextItems = pruneItems(listOf(item) + _state.value.items, _state.value.maxItems)
        persistItems(nextItems)
    }

    private fun persistItems(
        items: List<HistoryItem>,
        maxItems: Int = _state.value.maxItems,
    ) {
        persistence.saveDatabase(StoredHistoryDatabase(items = items))
        _state.value = _state.value.copy(items = items, maxItems = maxItems)
    }

    private fun pruneItems(
        items: List<HistoryItem>,
        maxItems: Int,
        deletePrunedFiles: Boolean = true,
    ): List<HistoryItem> {
        if (items.size <= maxItems) {
            return items
        }
        val keep = items.take(maxItems)
        if (deletePrunedFiles) {
            items.drop(maxItems).forEach(::deleteBackingFile)
        }
        return keep
    }

    private fun deleteBackingFile(item: HistoryItem) {
        runCatching {
            persistence.mediaFile(item.mediaPath).delete()
        }
    }
}
