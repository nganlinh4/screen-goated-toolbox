package dev.screengoated.toolbox.mobile.creation

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
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

internal class CreationNativeViewModel(
    application: Application,
    val tool: CreationTool,
    private val ownerId: String,
) : AndroidViewModel(application) {
    private val manager = CreationJobManager.get(application)
    private val previews = CreationPreviewFiles(manager.files)
    private val lifetime = CreationMiniAppLifetime()
    private val schedulerMutex = Mutex()
    private val surfaceAcquired = AtomicBoolean()
    private val statusMonitorLock = Any()
    private var statusMonitor: Job? = null
    private val mutableState = MutableStateFlow(
        CreationNativeUiState(outputDirectory = manager.files.defaultOutputDirectoryLabel()),
    )
    val state: StateFlow<CreationNativeUiState> = mutableState.asStateFlow()

    init {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.awaitStartup() }.onSuccess {
                recoverRunningJobs()
                refreshHistory()
            }
        }
        if (tool == CreationTool.IMAGE_CREATOR) addImageSession()
    }

    fun activateSurface() {
        if (!surfaceAcquired.compareAndSet(false, true) || lifetime.isClosed) return
        viewModelScope.launch(Dispatchers.IO) {
            manager.acquireSurface(tool, ownerId)
            mutableState.update { current ->
                current.copy(items = current.items.map(::withRuntimeCapabilities))
            }
            delay(1_000)
            mutableState.update { current ->
                current.copy(items = current.items.map(::withRuntimeCapabilities))
            }
        }
    }

    fun addImages(paths: List<String>) {
        if (paths.isEmpty()) return
        if (tool == CreationTool.IMAGE_CREATOR) {
            addImageReferences(paths)
            return
        }
        val batchId = "batch_${UUID.randomUUID()}"
        val additions = creationDraftsForImport(paths, batchId) {
            "image_${UUID.randomUUID()}"
        }
        mutableState.update {
            it.copy(
                tab = CreationNativeTab.JOBS,
                items = it.items + additions,
                selectedItemId = additions.first().id,
                selectedHistoryId = null,
                transientError = null,
            )
        }
        syncSourceHandles()
    }

    fun addImageSession() {
        if (tool != CreationTool.IMAGE_CREATOR) return
        val item = CreationImageSessions.new()
        mutableState.update {
            it.copy(
                tab = CreationNativeTab.JOBS,
                items = it.items + item,
                selectedItemId = item.id,
                selectedHistoryId = null,
                transientError = null,
            )
        }
    }

    fun removeImageReference(index: Int) {
        val selected = mutableState.value.selectedItem ?: return
        if (tool != CreationTool.IMAGE_CREATOR ||
            selected.submitted ||
            selected.stage != CreationNativeStage.DRAFT
        ) return
        updateItem(selected.id) { CreationImageSessions.removeReference(it, index) }
        syncSourceHandles()
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
        if (tab == CreationNativeTab.RESULTS) refreshHistory()
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
        syncSourceHandles()
    }

    fun setPolycount(value: Int) = updateSelectedConfigurable { item ->
        route3dItem(
            item.copy(
                polycount = value.coerceIn(
                    CreationContract.MINIMUM_POLYCOUNT,
                    CreationContract.MAXIMUM_POLYCOUNT,
                ),
            ),
        )
    }

    fun setGenerationMode(mode: String) = updateSelectedConfigurable { item ->
        route3dItem(
            item.copy(
                generationMode = CreationGenerationMode.fromWireName(mode).wireName,
            ),
        )
    }

    fun setInstruction(instruction: String) = updateSelectedConfigurable { item ->
        if (item.allowsInstruction) {
            item.copy(
                instruction = instruction.take(
                    CreationContract.MAXIMUM_OPTIONAL_INSTRUCTION_CHARACTERS,
                ),
            )
        } else {
            item
        }
    }

    fun setAutoSegment(enabled: Boolean) = updateSelectedConfigurable { item ->
        route3dItem(item.copy(autoSegment = enabled))
    }

    fun setModel(model: String) = updateSelectedConfigurable {
        it.copy(model = if (model == "detail") "detail" else "simple")
    }

    fun setSvgBackgroundMode(mode: String) = updateSelectedConfigurable {
        it.copy(backgroundMode = normalizeSvgBackgroundMode(mode))
    }

    fun setPrompt(prompt: String) = updateSelectedConfigurable {
        it.copy(prompt = prompt.take(CreationContract.IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS))
    }

    fun submitSelected() {
        val selected = mutableState.value.selectedItem ?: return
        if (tool == CreationTool.IMAGE_CREATOR && selected.prompt.isBlank()) {
            showError(IllegalArgumentException("Describe the image you want to create"))
            return
        }
        mutableState.update { current ->
            current.submitSelectedItem()
        }
        ensureStatusMonitor()
        schedule()
    }

    fun cancelSelected() {
        val selected = mutableState.value.selectedItem ?: return
        val jobId = selected.status?.jobId
        if (selected.stage == CreationNativeStage.RUNNING && jobId != null) {
            manager.cancel(ownerId, tool, jobId)
        }
        updateItem(selected.id) {
            it.copy(stage = CreationNativeStage.CANCELLED, submitted = true)
        }
        schedule()
    }

    fun closeMiniApp() {
        lifetime.close {
            mutableState.update(CreationNativeUiState::cancelActiveItems)
            try {
                manager.closeOwner(tool, ownerId)
            } finally {
                try {
                    manager.files.releaseSurfaceSources(ownerId)
                } finally {
                    if (surfaceAcquired.get()) manager.releaseSurface(tool, ownerId)
                }
            }
        }
    }

    fun segmentSelected() {
        val selected = mutableState.value.selectedItem ?: return
        val continuationId = selected.status?.jobId ?: return
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.startSegmentation(ownerId, continuationId) }
                .onSuccess { status ->
                    updateItem(selected.id) {
                        it.copy(
                            stage = CreationNativeStage.RUNNING,
                            status = status,
                            submitted = true,
                            generationMode = status.generationMode ?: it.generationMode,
                            polycount = status.polycount ?: it.polycount,
                            autoSegment = status.autoSegment ?: it.autoSegment,
                        )
                    }
                    ensureStatusMonitor()
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

    fun deleteAllHistory() {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.deleteAllHistory(tool) }
                .onSuccess {
                    mutableState.update { current ->
                        current.copy(history = emptyList(), selectedHistoryId = null)
                    }
                }
                .onFailure(::showError)
        }
    }

    fun openOutput(path: String) {
        runCatching { manager.files.openExternally(path) }.onFailure(::showError)
    }

    suspend fun previewFile(path: String, extension: String) = previews.materialize(path, extension)
    suspend fun viewerModelFile(path: String) = previews.viewerModel(path)
    fun releaseViewerModelFile(file: java.io.File) = previews.releaseViewerModel(file)
    suspend fun readSvg(path: String) = previews.readSvg(path)
    suspend fun saveSvg(path: String, svg: String) = previews.saveSvg(path, svg)

    fun dismissError() {
        mutableState.update { it.copy(transientError = null) }
    }

    private fun updateSelectedConfigurable(transform: (CreationNativeItem) -> CreationNativeItem) {
        val selected = mutableState.value.selectedItem ?: return
        val draft = !selected.submitted && selected.stage == CreationNativeStage.DRAFT
        if (!selected.isConfigurable()) return
        mutableState.update { current ->
            current.copy(
                items = current.items.map { item ->
                    val matches = if (draft) {
                        item.batchId == selected.batchId && !item.submitted
                    } else {
                        item.id == selected.id
                    }
                    if (matches) transform(item) else item
                },
            )
        }
    }

    private fun schedule() {
        viewModelScope.launch(Dispatchers.IO) {
            manager.awaitStartup()
            schedulerMutex.withLock {
                while (!lifetime.isClosed) {
                    val next = mutableState.value.items.firstOrNull {
                        it.submitted && it.stage == CreationNativeStage.QUEUED
                    } ?: break
                    val routed = if (tool == CreationTool.IMAGE_TO_3D) route3dItem(next) else next
                    if (routed != next) {
                        updateItem(next.id) { current ->
                            current.copy(
                                generationMode = routed.generationMode,
                                polycount = routed.polycount,
                                autoSegment = routed.autoSegment,
                            )
                        }
                    }
                    val args = creationSubmissionArgs(tool, routed)
                    val outcome = lifetime.computeIfOpen {
                        manager.startJob(ownerId, tool, args)
                    } ?: break
                    val status = when (outcome) {
                        is CreationSubmissionOutcome.Accepted -> outcome.status
                        is CreationSubmissionOutcome.Rejected -> {
                        updateItem(next.id) {
                            it.copy(
                                stage = CreationNativeStage.FAILED,
                                status = CreationJobStatus(
                                    stage = "failed",
                                    progressText = "Could not create result.",
                                    error = if (
                                        outcome.category ==
                                        CreationSubmissionFailure.STORAGE_UNAVAILABLE
                                    ) {
                                        CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
                                    } else if (
                                        outcome.category ==
                                        CreationSubmissionFailure.SOURCE_UNAVAILABLE
                                    ) {
                                        CREATION_SOURCE_UNAVAILABLE_ERROR_KEY
                                    } else {
                                        publicCreationFailure(tool)
                                    },
                                    sourceImagePath = routed.sourcePath,
                                    sourceImagePaths = routed.referencePaths,
                                    operation = CreationContract.IMAGE_CREATOR_OPERATION.takeIf {
                                        tool == CreationTool.IMAGE_CREATOR
                                    },
                                    prompt = routed.prompt.takeIf {
                                        tool == CreationTool.IMAGE_CREATOR
                                    },
                                    instruction = routed.instruction.takeIf {
                                        tool == CreationTool.IMAGE_TO_3D &&
                                            routed.allowsInstruction
                                    },
                                    generationMode = routed.generationMode,
                                    polycount = routed.polycount,
                                    autoSegment = routed.autoSegment,
                                ),
                            )
                        }
                        continue
                        }
                    }
                    var accepted = false
                    val published = lifetime.computeIfOpen {
                        mutableState.update { current ->
                            current.copy(
                                items = current.items.map { item ->
                                    if (item.id == next.id &&
                                        item.stage == CreationNativeStage.QUEUED &&
                                        item.submissionToken == next.submissionToken
                                    ) {
                                        accepted = true
                                        item.copy(
                                            stage = CreationNativeStage.RUNNING,
                                            status = status,
                                            generationMode = status.generationMode
                                                ?: routed.generationMode,
                                            autoSegment = status.autoSegment ?: routed.autoSegment,
                                        )
                                    } else {
                                        item
                                    }
                                },
                            )
                        }
                        true
                    }
                    if (published == null) break
                    if (!accepted) {
                        manager.cancel(ownerId, tool, status.jobId)
                        continue
                    }
                    ensureStatusMonitor()
                }
            }
        }
    }

    private fun ensureStatusMonitor() {
        if (lifetime.isClosed || !creationSurfaceHasActiveWork(mutableState.value.items)) return
        synchronized(statusMonitorLock) {
            if (statusMonitor?.isActive == true) return
            statusMonitor = viewModelScope.launch(Dispatchers.IO) {
                try {
                    while (!lifetime.isClosed &&
                        creationSurfaceHasActiveWork(mutableState.value.items)
                    ) {
                        refreshLiveStatuses()
                        mutableState.update { current ->
                            current.copy(
                                preparationStatus = manager.preparationStatus(tool),
                                items = current.items.map(::withRuntimeCapabilities),
                            )
                        }
                        if (creationSurfaceHasActiveWork(mutableState.value.items)) delay(1_000)
                    }
                } finally {
                    synchronized(statusMonitorLock) { statusMonitor = null }
                    if (creationSurfaceHasActiveWork(mutableState.value.items)) {
                        ensureStatusMonitor()
                    }
                }
            }
        }
    }

    private fun refreshLiveStatuses(): Boolean {
        if (lifetime.isClosed) return false
        val byJob = manager.statuses(ownerId, tool)
            .mapNotNull { status -> status.jobId?.let { it to status } }
            .toMap()
        val missingRunning = mutableState.value.items.any { item ->
            item.stage == CreationNativeStage.RUNNING &&
                item.status?.jobId?.let { it !in byJob } == true
        }
        val verifiedHistory = if (missingRunning) manager.history.list(tool) else null
        var reachedTerminal = false
        mutableState.update { current ->
            refreshCreationNativeItems(current, byJob, verifiedHistory, tool).also {
                reachedTerminal = it.reachedTerminal
            }.state
        }
        if (reachedTerminal) refreshHistoryNow()
        syncSourceHandles()
        return byJob.values.any { it.toNativeStage() == CreationNativeStage.RUNNING }
    }

    private fun recoverRunningJobs() {
        viewModelScope.launch(Dispatchers.IO) {
            val recovered = manager.statuses(ownerId, tool).filter {
                it.toNativeStage() != CreationNativeStage.CANCELLED &&
                    (tool == CreationTool.IMAGE_CREATOR || !it.sourceImagePath.isNullOrBlank())
            }
            if (recovered.isEmpty()) return@launch
            val items = recovered.map { status ->
                val references = if (tool == CreationTool.IMAGE_CREATOR) {
                    CreationImageSessions.statusReferences(status)
                } else {
                    listOf(requireNotNull(status.sourceImagePath))
                }
                val path = references.firstOrNull().orEmpty()
                val polycount = status.polycount ?: CreationContract.DEFAULT_POLYCOUNT
                val autoSegment = status.autoSegment ?: false
                val generationMode = CreationGenerationMode
                    .fromWireName(status.generationMode)
                CreationNativeItem(
                    id = status.jobId ?: "recovered_${UUID.randomUUID()}",
                    batchId = "recovered_${status.jobId}",
                    sourcePath = path,
                    sourceName = path.takeIf(String::isNotBlank)?.let(::File)?.name.orEmpty(),
                    referencePaths = references,
                    generationMode = generationMode.wireName,
                    polycount = polycount,
                    model = status.model ?: "simple",
                    backgroundMode = normalizeSvgBackgroundMode(status.backgroundMode),
                    prompt = status.prompt.orEmpty(),
                    instruction = status.instruction.orEmpty(),
                    allowsInstruction = manager.supportsOptionalInstruction(
                        generationMode.wireName,
                    ),
                    autoSegment = autoSegment,
                    submitted = true,
                    stage = status.toNativeStage(),
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
            syncSourceHandles()
            ensureStatusMonitor()
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
        if (lifetime.isClosed) return
        mutableState.update { current ->
            current.copy(items = current.items.map { if (it.id == id) transform(it) else it })
        }
    }

    private fun syncSourceHandles() {
        val retained = creationVisibleSessionSourceHandles(mutableState.value.items)
        manager.files.updateSurfaceSources(ownerId, retained)
    }

    private fun addImageReferences(paths: List<String>) {
        mutableState.update { CreationImageSessions.addReferences(it, paths) }
        syncSourceHandles()
    }

    private fun route3dItem(item: CreationNativeItem): CreationNativeItem {
        return routeCreationNativeItem(tool, item, manager::supportsOptionalInstruction)
    }

    private fun withRuntimeCapabilities(item: CreationNativeItem): CreationNativeItem {
        return applyCreationRuntimeCapabilities(tool, item, manager::supportsOptionalInstruction)
    }

    internal fun showError(error: Throwable) {
        if (lifetime.isClosed) return
        mutableState.update { it.copy(transientError = publicCreationThrowable(error, tool)) }
    }

    override fun onCleared() {
        closeMiniApp()
        super.onCleared()
    }
}
