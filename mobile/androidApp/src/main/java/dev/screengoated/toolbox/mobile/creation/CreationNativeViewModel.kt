package dev.screengoated.toolbox.mobile.creation

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
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
    private val projects by lazy {
        CreationNativeProjectLauncher(
            application, manager, tool, viewModelScope, mutableState, ::showError,
        )
    }
    private val segmentations = CreationNativeSegmentationLauncher(
        manager,
        ownerId,
        viewModelScope,
        { parent, status -> projects.publishChild(parent, status) },
        ::ensureStatusMonitor,
        ::showError,
    )
    private var statusMonitor: Job? = null
    private val mutableState = MutableStateFlow(
        CreationNativeUiState(outputDirectory = manager.files.defaultOutputDirectoryLabel(tool)),
    )
    val state: StateFlow<CreationNativeUiState> = mutableState.asStateFlow()
    private val inputPreflight = CreationInputPreflight(manager.files, tool, viewModelScope, mutableState,
        { lifetime.isClosed }, { ensureStatusMonitor(); schedule() })

    init {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.awaitStartup() }.onSuccess {
                recoverRunningJobs()
                refreshHistory()
            }
        }
    }

    fun activateSurface() {
        if (!surfaceAcquired.compareAndSet(false, true) || lifetime.isClosed) return
        viewModelScope.launch(Dispatchers.IO) {
            manager.acquireSurface(tool, ownerId)
            while (!lifetime.isClosed) {
                val readiness = manager.preparationStatus(tool)
                val modes = manager.generationModes()
                val capabilities = runtimeCapabilitiesForTick()
                mutableState.update { current ->
                    current.copy(preparationStatus = readiness, generationModes = modes, items = current.items.map(capabilities))
                }
                delay(2_000)
            }
        }
    }

    fun addImages(paths: List<String>) {
        if (paths.isEmpty()) return
        val batchId = "batch_${UUID.randomUUID()}"
        val additions = creationDraftsForImport(paths, batchId) {
            "image_${UUID.randomUUID()}"
        }
        inputPreflight.updateInput {
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

    fun selectItem(id: String) {
        inputPreflight.updateInput {
            it.copy(tab = CreationNativeTab.JOBS, selectedItemId = id, selectedHistoryId = null)
        }
    }

    fun selectHistory(id: String) {
        inputPreflight.updateInput {
            it.copy(tab = CreationNativeTab.RESULTS, selectedHistoryId = id, selectedItemId = null)
        }
    }

    fun showTab(tab: CreationNativeTab) {
        inputPreflight.updateInput { current ->
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
        inputPreflight.updateInput { current ->
            val item = current.items.firstOrNull { it.id == id }
            if (item?.stage == CreationNativeStage.RUNNING) return@updateInput current
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
                generationMode = CreationContract.newGenerationMode(mode).wireName,
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

    fun setTopology(topology: String) = updateSelectedConfigurable { item ->
        route3dItem(item.copy(topology = topology, autoSegment = item.autoSegment && topology != "quad"))
    }

    fun setSegmentationLevel(level: String) = updateSelectedConfigurable { item ->
        item.copy(
            segmentationLevel = level.takeIf {
                it == "simple" || it == "balanced" || it == "detailed"
            } ?: "detailed",
        )
    }

    fun setModel(model: String) = updateSelectedConfigurable {
        it.copy(model = if (model == "detail") "detail" else "simple")
    }

    fun setSvgBackgroundMode(mode: String) = updateSelectedConfigurable {
        it.copy(backgroundMode = normalizeSvgBackgroundMode(mode))
    }

    fun submitSelected() = submitCreationSelection(tool, mutableState, inputPreflight, ::showError)

    fun dismissInputValidation(validation: CreationInputValidation): Boolean = inputPreflight.dismiss(validation)
    fun retryInputValidation(validation: CreationInputValidation) = inputPreflight.retry(validation)

    // manager.cancel takes the mutation lock and fsyncs the journal; never on Main.
    fun cancelSelected() {
        viewModelScope.launch(Dispatchers.IO) {
            cancelCreationSelection(mutableState, inputPreflight,
                { jobId -> manager.cancel(ownerId, tool, jobId) }, ::showError,
                { ensureStatusMonitor(); schedule() })
        }
    }

    fun closeMiniApp() {
        inputPreflight.close()
        lifetime.close {
            mutableState.update(CreationNativeUiState::cancelActiveItems)
            // Runs from onDestroy/onCleared: hand the locked, disk-bound teardown to a
            // scope that outlives this ViewModel instead of blocking Main.
            creationSurfaceIoScope.launch {
                runCatching {
                    releaseCreationOwner(
                        closeOwner = { manager.closeOwner(tool, ownerId) },
                        releaseSources = { manager.files.releaseSurfaceSources(ownerId) },
                        releaseSurface = {
                            if (surfaceAcquired.get()) manager.releaseSurface(tool, ownerId)
                        },
                    )
                }
            }
        }
    }

    fun segmentSelected() {
        mutableState.value.selectedItem?.let(segmentations::startManual)
    }

    fun refineSelected(kind: String, targetFaces: Int? = null, preset: String? = null) {
        val item = mutableState.value.selectedItem ?: return
        if (CreationContract.refinementCapability(kind) !in
            item.status?.availableActions.orEmpty()
        ) return
        segmentations.startManual(item, kind, targetFaces, preset)
    }

    fun rememberOutputDirectory(uri: Uri) {
        inputPreflight.invalidate()
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { manager.files.rememberOutputDirectory(uri) }
                .onSuccess { label -> inputPreflight.updateInput { it.copy(outputDirectory = label) } }
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
                .onFailure { error ->
                    // A partial clear deleted some entries; show what remains.
                    showError(error)
                    runCatching { refreshHistoryNow() }
                }
        }
    }

    fun openOutput(path: String) {
        runCatching { manager.files.openExternally(path) }.onFailure(::showError)
    }

    fun exportSelectedRevision() = projects.exportSelectedRevision()

    suspend fun previewFile(path: String, extension: String) = previews.materialize(path, extension)
    suspend fun viewerModelFile(path: String) = previews.viewerModel(path)
    fun releaseViewerModelFile(file: java.io.File) = previews.releaseViewerModel(file)
    suspend fun readSvg(path: String) = previews.readSvg(path)
    suspend fun saveSvg(path: String, svg: String) {
        withContext(Dispatchers.IO) {
            val updated = manager.history.saveSvgEdit(path, svg)
            val history = runCatching { manager.history.list(tool) }.getOrNull()
            mutableState.update { it.withSavedSvgEdit(path, updated, history) }
        }
    }

    fun dismissError() {
        mutableState.update { it.copy(transientError = null) }
    }

    private fun updateSelectedConfigurable(transform: (CreationNativeItem) -> CreationNativeItem) {
        val selected = mutableState.value.selectedItem ?: return
        val draft = !selected.submitted && selected.stage == CreationNativeStage.DRAFT
        if (!selected.isConfigurable()) return
        inputPreflight.updateInput { current ->
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
                        it.submitted && it.stage == CreationNativeStage.QUEUED && it.status?.jobId.isNullOrBlank()
                    } ?: break
                    val routed = if (tool == CreationTool.IMAGE_TO_3D) route3dItem(next) else next
                    if (routed != next) {
                        updateItem(next.id) { current ->
                            current.copy(
                                generationMode = routed.generationMode,
                                polycount = routed.polycount,
                                autoSegment = routed.autoSegment,
                                segmentationLevel = routed.segmentationLevel,
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
                                        publicCreationFailure()
                                    },
                                    sourceImagePath = routed.sourcePath,
                                    sourceImagePaths = routed.referencePaths,
                                    generationMode = routed.generationMode,
                                    polycount = routed.polycount,
                                    autoSegment = routed.autoSegment,
                                    segmentationLevel = routed.segmentationLevel,
                                ),
                            )
                        }
                        continue
                        }
                    }
                    val published = lifetime.computeIfOpen {
                        publishCreationSubmission(mutableState, next, routed, status)
                    }
                    if (published == null) break
                    if (!published) {
                        lifetime.computeIfOpen {
                            reconcileRetiredCreationSubmission(mutableState, next, status,
                                { jobId -> manager.cancel(ownerId, tool, jobId) }, ::showError)
                        }
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
                        val readiness = manager.preparationStatus(tool)
                        val capabilities = runtimeCapabilitiesForTick()
                        mutableState.update { current ->
                            current.copy(
                                preparationStatus = readiness,
                                items = current.items.map(capabilities),
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
            refreshCreationNativeItems(current, byJob, verifiedHistory).also {
                reachedTerminal = it.reachedTerminal
            }.state
        }
        if (reachedTerminal) refreshHistoryNow()
        segmentations.startPending(mutableState.value.items)
        syncSourceHandles()
        return byJob.values.any { it.toNativeStage() == CreationNativeStage.RUNNING }
    }

    private fun recoverRunningJobs() {
        viewModelScope.launch(Dispatchers.IO) {
            val recovered = manager.statuses(ownerId, tool).filter {
                it.toNativeStage() != CreationNativeStage.CANCELLED &&
                    !it.sourceImagePath.isNullOrBlank()
            }
            if (recovered.isEmpty()) return@launch
            val items = recovered.map { recoverCreationNativeItem(tool, it, manager::supportsOptionalInstruction) }
            mutableState.update { current ->
                current.copy(
                    items = current.items + items.filter { item ->
                        current.items.none { it.status?.jobId == item.status?.jobId }
                    },
                    selectedItemId = current.selectedItemId ?: items.firstOrNull()?.id,
                )
            }
            syncSourceHandles()
            segmentations.startPending(mutableState.value.items)
            ensureStatusMonitor()
        }
    }

    private fun refreshHistory() {
        viewModelScope.launch(Dispatchers.IO) { refreshHistoryNow() }
    }

    private fun refreshHistoryNow() {
        val entries = manager.history.list(tool)
        val pendingEdits = manager.history.hasUnresolvedSvgEdits()
        mutableState.update { current ->
            current.copy(
                history = entries,
                svgRecoveryPending = pendingEdits,
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
        val cleanup = manager.files.updateSurfaceSourceLeases(ownerId, retained)
        creationSurfaceIoScope.launch { runCatching { cleanup.run() } }
    }

    private fun route3dItem(item: CreationNativeItem): CreationNativeItem {
        return routeCreationNativeItem(tool, item, manager::supportsOptionalInstruction)
    }

    /**
     * Built once per status tick, outside the StateFlow update lambda: the runtime
     * capability read parses the runtime manifest, so it is memoized per mode.
     */
    private fun runtimeCapabilitiesForTick(): (CreationNativeItem) -> CreationNativeItem {
        val supports = creationMemoizedInstructionSupport(manager::supportsOptionalInstruction)
        return { item -> applyCreationRuntimeCapabilities(tool, item, supports) }
    }

    internal fun showError(error: Throwable) {
        if (lifetime.isClosed) return
        mutableState.update { it.copy(transientError = publicCreationThrowable(error)) }
    }

    override fun onCleared() {
        closeMiniApp()
        super.onCleared()
    }
}
