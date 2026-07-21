package dev.screengoated.toolbox.mobile.creation

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import java.io.File
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class CreationNativeViewModel(
    application: Application,
    val tool: CreationTool,
) : AndroidViewModel(application) {
    private val manager = CreationJobManager.get(application)
    private val schedulerMutex = Mutex()
    private val monitors = ConcurrentHashMap<String, Job>()
    private val mutableState = MutableStateFlow(
        CreationNativeUiState(outputDirectory = manager.files.defaultOutputDirectoryLabel()),
    )
    val state: StateFlow<CreationNativeUiState> = mutableState.asStateFlow()

    init {
        manager.startPreparation(tool)
        recoverRunningJobs()
        refreshHistory()
        viewModelScope.launch(Dispatchers.IO) {
            while (true) {
                mutableState.update { it.copy(preparationStatus = manager.preparationStatus(tool)) }
                delay(if (manager.preparationStatus(tool) == "ready") 15_000 else 1_500)
            }
        }
        viewModelScope.launch(Dispatchers.IO) {
            while (true) {
                delay(5_000)
                refreshHistoryNow()
            }
        }
    }

    fun addImages(paths: List<String>) {
        if (paths.isEmpty()) return
        val existing = mutableState.value.items.map { it.sourcePath.lowercase() }.toMutableSet()
        val batchId = "batch_${UUID.randomUUID()}"
        val additions = paths.filter { existing.add(it.lowercase()) }.map { path ->
            CreationNativeItem(
                id = "image_${UUID.randomUUID()}",
                batchId = batchId,
                sourcePath = path,
                sourceName = File(path).name,
            )
        }
        if (additions.isEmpty()) return
        mutableState.update {
            it.copy(
                tab = CreationNativeTab.JOBS,
                items = it.items + additions,
                selectedItemId = additions.first().id,
                selectedHistoryId = null,
                transientError = null,
            )
        }
    }

    fun selectItem(id: String) {
        mutableState.update {
            it.copy(tab = CreationNativeTab.JOBS, selectedItemId = id, selectedHistoryId = null)
        }
    }

    fun selectHistory(id: String) {
        mutableState.update {
            it.copy(tab = CreationNativeTab.RESULTS, selectedHistoryId = id, selectedItemId = null)
        }
    }

    fun showTab(tab: CreationNativeTab) {
        mutableState.update { current ->
            current.copy(
                tab = tab,
                selectedItemId = if (tab == CreationNativeTab.JOBS) {
                    current.selectedItemId ?: current.items.firstOrNull()?.id
                } else null,
                selectedHistoryId = if (tab == CreationNativeTab.RESULTS) {
                    current.selectedHistoryId ?: current.history.firstOrNull()?.id
                } else null,
            )
        }
    }

    fun removeDraft(id: String) {
        mutableState.update { current ->
            val item = current.items.firstOrNull { it.id == id }
            if (item?.stage == CreationNativeStage.RUNNING) return@update current
            val remaining = current.items.filterNot { it.id == id }
            current.copy(
                items = remaining,
                selectedItemId = if (current.selectedItemId == id) remaining.firstOrNull()?.id
                else current.selectedItemId,
            )
        }
    }

    fun setPolycount(value: Int) = updateSelectedDraftBatch { item ->
        item.copy(
            polycount = value.coerceIn(
                CreationContract.MINIMUM_POLYCOUNT,
                CreationContract.MAXIMUM_POLYCOUNT,
            ),
        )
    }

    fun setAutoSegment(enabled: Boolean) = updateSelectedDraftBatch {
        it.copy(autoSegment = enabled)
    }

    fun setModel(model: String) = updateSelectedDraftBatch {
        it.copy(model = if (model == "detail") "detail" else "simple")
    }

    fun submitSelected() {
        val selected = mutableState.value.selectedItem ?: return
        mutableState.update { current ->
            val ids = if (!selected.submitted && selected.stage == CreationNativeStage.DRAFT) {
                current.items.filter { it.batchId == selected.batchId && !it.submitted }.map { it.id }.toSet()
            } else {
                setOf(selected.id)
            }
            current.copy(
                items = current.items.map { item ->
                    if (item.id in ids) {
                        item.copy(
                            submitted = true,
                            stage = CreationNativeStage.QUEUED,
                            status = null,
                        )
                    } else item
                },
                transientError = null,
            )
        }
        schedule()
    }

    fun cancelSelected() {
        val selected = mutableState.value.selectedItem ?: return
        val jobId = selected.status?.jobId
        if (selected.stage == CreationNativeStage.RUNNING && jobId != null) {
            manager.cancel(tool, jobId)
        }
        updateItem(selected.id) {
            it.copy(stage = CreationNativeStage.CANCELLED, submitted = true)
        }
        schedule()
    }

    fun segmentSelected() {
        val selected = mutableState.value.selectedItem ?: return
        val continuationId = selected.status?.jobId ?: return
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.startSegmentation(continuationId) }
                .onSuccess { status ->
                    updateItem(selected.id) {
                        it.copy(stage = CreationNativeStage.RUNNING, status = status, submitted = true)
                    }
                    monitor(selected.id, status)
                }
                .onFailure(::showError)
        }
    }

    fun rememberOutputDirectory(uri: Uri) {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.files.rememberOutputDirectory(uri) }
                .onSuccess { label -> mutableState.update { it.copy(outputDirectory = label) } }
                .onFailure(::showError)
        }
    }

    fun renameHistory(id: String, name: String) {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.renameHistory(tool, id, name) }
                .onSuccess { updated ->
                    mutableState.update { current ->
                        current.copy(history = current.history.map { if (it.id == id) updated else it })
                    }
                }
                .onFailure(::showError)
        }
    }

    fun deleteHistory(id: String) {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.deleteHistory(tool, id) }
                .onSuccess {
                    mutableState.update { current ->
                        val remaining = current.history.filterNot { it.id == id }
                        current.copy(
                            history = remaining,
                            selectedHistoryId = if (current.selectedHistoryId == id) {
                                remaining.firstOrNull()?.id
                            } else current.selectedHistoryId,
                        )
                    }
                }
                .onFailure(::showError)
        }
    }

    fun openOutput(path: String) {
        runCatching { manager.files.openExternally(path) }.onFailure(::showError)
    }

    suspend fun previewFile(path: String, extension: String): File = withContext(Dispatchers.IO) {
        manager.files.materializePreview(path, extension)
    }

    suspend fun readSvg(path: String): String = withContext(Dispatchers.IO) {
        manager.files.readBytes(path, 20L * 1024 * 1024).decodeToString()
    }

    suspend fun saveSvg(path: String, svg: String) = withContext(Dispatchers.IO) {
        require(svg.length <= 20 * 1024 * 1024) { "Edited SVG is too large" }
        val lower = svg.lowercase()
        require(
            lower.contains("<svg") && lower.contains("</svg>") &&
                listOf("<script", "<foreignobject", "javascript:", " onload=", " onerror=")
                    .none(lower::contains),
        ) { "Edited SVG contains unsupported active content" }
        manager.files.writeText(path, svg)
    }

    fun dismissError() {
        mutableState.update { it.copy(transientError = null) }
    }

    private fun updateSelectedDraftBatch(transform: (CreationNativeItem) -> CreationNativeItem) {
        val selected = mutableState.value.selectedItem ?: return
        if (selected.submitted || selected.stage != CreationNativeStage.DRAFT) return
        mutableState.update { current ->
            current.copy(
                items = current.items.map { item ->
                    if (item.batchId == selected.batchId && !item.submitted) transform(item) else item
                },
            )
        }
    }

    private fun schedule() {
        viewModelScope.launch(Dispatchers.IO) {
            schedulerMutex.withLock {
                while (mutableState.value.runningCount < CreationContract.MAXIMUM_PARALLEL_JOBS) {
                    val next = mutableState.value.items.firstOrNull {
                        it.submitted && it.stage == CreationNativeStage.QUEUED
                    } ?: break
                    val args = buildJsonObject {
                        put("imagePath", next.sourcePath)
                        put("polycount", next.polycount)
                        put("autoSegment", next.autoSegment)
                        put("segmentationMode", if (next.autoSegment) "parts" else "none")
                        put("model", next.model)
                    }
                    val status = runCatching { manager.startJob(tool, args) }.getOrElse { error ->
                        if (error.message.orEmpty().contains("busy", ignoreCase = true)) {
                            viewModelScope.launch {
                                delay(1_000)
                                schedule()
                            }
                            return@withLock
                        }
                        updateItem(next.id) {
                            it.copy(
                                stage = CreationNativeStage.FAILED,
                                status = CreationJobStatus(
                                    stage = "failed",
                                    progressText = "Could not create result.",
                                    error = error.message,
                                    sourceImagePath = next.sourcePath,
                                ),
                            )
                        }
                        continue
                    }
                    updateItem(next.id) {
                        it.copy(stage = CreationNativeStage.RUNNING, status = status)
                    }
                    monitor(next.id, status)
                }
            }
        }
    }

    private fun monitor(itemId: String, initial: CreationJobStatus) {
        val jobId = initial.jobId ?: return
        monitors.remove(jobId)?.cancel()
        monitors[jobId] = viewModelScope.launch(Dispatchers.IO) {
            var status = initial
            while (status.toNativeStage() == CreationNativeStage.RUNNING) {
                delay(1_000)
                status = manager.status(tool, jobId)
                updateItem(itemId) { it.copy(stage = status.toNativeStage(), status = status) }
            }
            monitors.remove(jobId)
            refreshHistoryNow()
            schedule()
        }
    }

    private fun recoverRunningJobs() {
        viewModelScope.launch(Dispatchers.IO) {
            val recovered = manager.statuses(tool).filter {
                it.sourceImagePath != null && it.toNativeStage() == CreationNativeStage.RUNNING
            }
            if (recovered.isEmpty()) return@launch
            val items = recovered.map { status ->
                val path = requireNotNull(status.sourceImagePath)
                CreationNativeItem(
                    id = status.jobId ?: "recovered_${UUID.randomUUID()}",
                    batchId = "recovered_${status.jobId}",
                    sourcePath = path,
                    sourceName = File(path).name,
                    model = status.model ?: "simple",
                    autoSegment = status.isSegmented,
                    submitted = true,
                    stage = CreationNativeStage.RUNNING,
                    status = status,
                )
            }
            mutableState.update { current ->
                current.copy(
                    items = current.items + items.filter { item ->
                        current.items.none { it.status?.jobId == item.status?.jobId }
                    },
                    selectedItemId = current.selectedItemId ?: items.firstOrNull()?.id,
                )
            }
            items.forEach { item -> item.status?.let { monitor(item.id, it) } }
        }
    }

    private fun refreshHistory() {
        viewModelScope.launch(Dispatchers.IO) { refreshHistoryNow() }
    }

    private fun refreshHistoryNow() {
        val entries = manager.history.list(tool)
        mutableState.update { current ->
            current.copy(
                history = entries,
                selectedHistoryId = current.selectedHistoryId?.takeIf { selected ->
                    entries.any { it.id == selected }
                } ?: if (current.tab == CreationNativeTab.RESULTS) entries.firstOrNull()?.id else null,
            )
        }
    }

    private fun updateItem(id: String, transform: (CreationNativeItem) -> CreationNativeItem) {
        mutableState.update { current ->
            current.copy(items = current.items.map { if (it.id == id) transform(it) else it })
        }
    }

    private fun showError(error: Throwable) {
        mutableState.update { it.copy(transientError = error.message ?: "Creation failed") }
    }

    internal class Factory(
        private val application: Application,
        private val tool: CreationTool,
    ) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T =
            CreationNativeViewModel(application, tool) as T
    }
}
