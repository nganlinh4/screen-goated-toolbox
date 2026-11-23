package dev.screengoated.toolbox.mobile.history

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.cancel
import java.io.File
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import java.util.concurrent.atomic.AtomicLong
import java.util.Collections

class HistoryRepository internal constructor(
    private val persistence: HistoryStorage,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    deferStartupPrune: Boolean = false,
    private val onMaxItemsChanged: () -> Unit = {},
) {
    private val scope = CoroutineScope(SupervisorJob() + ioDispatcher)
    private val admission = Any()
    private var saveEpoch = Any()
    private val queue = Channel<Change>(Channel.UNLIMITED)
    private val startupReady = CompletableDeferred<Unit>()
    private val nextId = AtomicLong(System.currentTimeMillis())
    private val timestampFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss")
    private val _state = MutableStateFlow(loadInitialState())
    val state: StateFlow<HistoryUiState> = _state.asStateFlow()

    init {
        queue.trySend(Change.Prune)
        if (!deferStartupPrune) startupReady.complete(Unit)
        scope.launch {
            startupReady.await()
            for (change in queue) {
                if (_state.value.storageUnavailable) continue
                try {
                    apply(change)
                } catch (_: Exception) {
                    publishState { it.copy(storageError = HistoryStorageError.WRITE_FAILED) }
                }
            }
        }
    }

    fun saveText(resultText: String, inputText: String) {
        if (resultText.isBlank()) return
        save(HistoryType.TEXT, resultText, inputText.toByteArray(Charsets.UTF_8))
    }

    fun saveImage(pngBytes: ByteArray, resultText: String) {
        if (resultText.isBlank() || pngBytes.isEmpty()) return
        save(HistoryType.IMAGE, resultText, pngBytes.copyOf())
    }

    fun saveAudio(wavBytes: ByteArray, resultText: String) {
        if (resultText.isBlank() || wavBytes.isEmpty()) return
        save(HistoryType.AUDIO, resultText, wavBytes.copyOf())
    }

    private fun save(type: HistoryType, text: String, bytes: ByteArray) {
        synchronized(admission) {
            queue.trySend(Change.Save(type, text, bytes, saveEpoch))
        }
    }

    fun delete(id: Long) {
        synchronized(admission) {
            val matches = _state.value.items.filter { it.id == id }
            when (matches.size) {
                0 -> Unit
                1 -> queue.trySend(Change.Delete(matches.single()))
                else -> queue.trySend(Change.Reject)
            }
        }
    }

    fun clearAll() {
        synchronized(admission) {
            saveEpoch = Any()
            queue.trySend(Change.Clear)
        }
    }

    fun updateMaxItems(value: Int) {
        enqueue(Change.Settings(HistorySettings(maxItems = clampHistoryLimit(value), hasExplicitMaxItems = true)))
    }

    fun resetSettingsToDefaults() = enqueue(Change.Settings(HistorySettings()))

    private fun enqueue(change: Change) {
        synchronized(admission) { queue.trySend(change) }
    }

    fun mediaFileFor(item: HistoryItem): File? {
        val current = state.value
        if (current.storageUnavailable) return null
        val items = current.items
        if (items.count { it.id == item.id } != 1 || item !in items) return null
        val file = persistence.mediaFile(item)
        return file.takeIf { state.value === current }
    }

    fun mediaProofForOpen(item: HistoryItem): HistoryMediaProof? {
        val current = state.value
        if (current.storageUnavailable || current.items.count { it.id == item.id } != 1 || item !in current.items) return null
        val proof = persistence.mediaProofForOpen(item)
        return proof.takeIf { state.value === current }
    }

    fun mediaDirectory(): File = persistence.paths().mediaDir

    internal fun <T> admitMediaOpen(snapshot: HistoryUiState, handoff: () -> T): T? = synchronized(admission) {
        if (_state.value !== snapshot) null else handoff()
    }

    private fun publishState(update: (HistoryUiState) -> HistoryUiState) {
        synchronized(admission) { _state.value = update(_state.value) }
    }

    internal fun completeDeferredStartup() { startupReady.complete(Unit) }

    internal fun cancelDeferredStartup() {
        check(!startupReady.isCompleted) { "History startup already completed" }
        queue.cancel()
        startupReady.cancel()
        scope.cancel()
    }

    private fun loadInitialState(): HistoryUiState {
        val database = runCatching { persistence.loadDatabase() }
        val settings = runCatching { persistence.loadSettings() }
        val unavailable = database.isFailure || settings.isFailure
        return HistoryUiState(
            items = immutableItems(database.getOrNull()?.items.orEmpty()),
            maxItems = settings.getOrNull()?.maxItems ?: DEFAULT_HISTORY_LIMIT,
            mediaDirectoryPath = persistence.paths().mediaDir.absolutePath,
            supportsFolderOpen = persistence.paths().supportsFolderOpen,
            storageError = if (unavailable) HistoryStorageError.LOAD_FAILED else null,
            storageUnavailable = unavailable,
        )
    }

    private fun apply(change: Change) {
        val current = _state.value
        when (change) {
            is Change.Save -> saveCommitted(change)
            is Change.Delete -> {
                if (current.items.count { it.id == change.item.id } != 1 || change.item !in current.items) {
                    apply(Change.Reject)
                } else commit(current.items.filterNot { it == change.item })
            }
            Change.Clear -> commit(emptyList())
            Change.Prune -> {
                if (current.items.size > current.maxItems) commit(current.items.take(current.maxItems))
            }
            is Change.Settings -> {
                if (writeResult { persistence.saveSettings(change.value) } != HistoryWriteResult.COMMITTED) return
                publishState { it.copy(maxItems = change.value.maxItems, storageError = null) }
                apply(Change.Prune)
                onMaxItemsChanged()
            }
            Change.Reject -> publishState { it.copy(storageError = HistoryStorageError.WRITE_FAILED) }
        }
    }

    private fun saveCommitted(change: Change.Save) {
        if (!synchronized(admission) { change.epoch === saveEpoch }) return
        persistence.createMedia(change.type, change.bytes).use { media ->
            val current = _state.value
            var id = nextId.incrementAndGet()
            while (current.items.any { it.id == id }) id = nextId.incrementAndGet()
            val item = HistoryItem(id, timestampFormatter.format(LocalDateTime.now()), change.type,
                change.text, media.name, media.proof)
            val snapshot = (listOf(item) + current.items).take(current.maxItems)
            media.revalidate()
            // Admission remains short; media validation and persistence hold no admission/cache lock.
            if (!synchronized(admission) { change.epoch === saveEpoch }) {
                persistence.removeMedia(item)
                return
            }
            if (commit(snapshot) { media.revalidate() } == HistoryWriteResult.REJECTED) persistence.removeMedia(item)
        }
    }

    private fun commit(items: List<HistoryItem>, verifyCommitted: () -> Unit = {}): HistoryWriteResult {
        val previous = _state.value.items
        val result = writeResult { persistence.saveDatabase(StoredHistoryDatabase(items = items)) }
        if (result != HistoryWriteResult.COMMITTED) return result
        val verified = writeResult { verifyCommitted(); HistoryWriteResult.COMMITTED }
        if (verified != HistoryWriteResult.COMMITTED) return verified
        publishState { it.copy(items = immutableItems(items), storageError = null) }
        previous.filter { old -> items.none { it.mediaPath == old.mediaPath } }.forEach(persistence::removeMedia)
        return result
    }

    private fun writeResult(write: () -> HistoryWriteResult): HistoryWriteResult {
        val result = try { write() } catch (_: Exception) { HistoryWriteResult.UNAVAILABLE }
        if (result != HistoryWriteResult.COMMITTED) {
            val unavailable = result == HistoryWriteResult.UNAVAILABLE
            publishState { it.copy(
                storageError = if (unavailable) HistoryStorageError.STORAGE_CHANGED else HistoryStorageError.WRITE_FAILED,
                storageUnavailable = unavailable,
            ) }
        }
        return result
    }

    private fun immutableItems(items: List<HistoryItem>): List<HistoryItem> =
        Collections.unmodifiableList(ArrayList(items))

    private sealed interface Change {
        data class Save(val type: HistoryType, val text: String, val bytes: ByteArray, val epoch: Any) : Change
        data class Delete(val item: HistoryItem) : Change
        data class Settings(val value: HistorySettings) : Change
        data object Clear : Change
        data object Prune : Change
        data object Reject : Change
    }
}
