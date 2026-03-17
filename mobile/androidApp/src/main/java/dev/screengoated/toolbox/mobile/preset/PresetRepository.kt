package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class PresetRepository(
    private val textApiClient: TextApiClient,
    private val apiKeys: () -> ApiKeys,
    private val runtimeSettings: () -> PresetRuntimeSettings,
    private val uiLanguage: () -> String,
    private val overrideStore: PresetOverrideStore,
    mainDispatcher: CoroutineDispatcher = Dispatchers.Main,
) {
    private val scope = CoroutineScope(SupervisorJob() + mainDispatcher)
    private val canonicalPresets = DefaultPresets.all
    private val canonicalById = canonicalPresets.associateBy { it.id }

    private val _catalogState = MutableStateFlow(PresetCatalogState())
    val catalogState: StateFlow<PresetCatalogState> = _catalogState.asStateFlow()

    private val _executionState = MutableStateFlow(PresetExecutionState())
    val executionState: StateFlow<PresetExecutionState> = _executionState.asStateFlow()

    private var storedOverrides = overrideStore.load()
    private var executionJob: Job? = null
    private var nextSessionOrdinal = 1L

    init {
        publishCatalog()
    }

    fun getAllPresets(): List<Preset> = _catalogState.value.presets.map { it.preset }

    fun getPresetsByType(type: PresetType): List<Preset> =
        _catalogState.value.presetsFor(type).map { it.preset }

    fun getResolvedPreset(id: String): ResolvedPreset? = _catalogState.value.findPreset(id)

    fun toggleFavorite(id: String) {
        updateBuiltInOverride(id) { it.copy(isFavorite = !it.isFavorite) }
    }

    fun restoreBuiltInPreset(id: String) {
        if (!canonicalById.containsKey(id)) {
            return
        }
        val updated = storedOverrides.builtInOverrides.toMutableMap().also { it.remove(id) }
        persistOverrides(updated)
    }

    fun updateBuiltInOverride(
        id: String,
        mutation: (Preset) -> Preset,
    ) {
        val canonical = canonicalById[id] ?: return
        val current = getResolvedPreset(id)?.preset ?: canonical
        val updatedPreset = mutation(current)
        val override = updatedPreset.toOverrideComparedTo(canonical)
        val updatedOverrides = storedOverrides.builtInOverrides.toMutableMap()
        if (override.isEmpty()) {
            updatedOverrides.remove(id)
        } else {
            updatedOverrides[id] = override
        }
        persistOverrides(updatedOverrides)
    }

    fun executePreset(
        preset: Preset,
        input: PresetInput,
    ) {
        val capability = resolveExecutionCapability(preset)
        if (!capability.supported) {
            _executionState.value = PresetExecutionState(
                activePresetId = preset.id,
                error = capability.reason?.message() ?: "Preset execution is not ready on Android yet.",
            )
            return
        }

        executionJob?.cancel()
        val sessionId = "preset-session-${nextSessionOrdinal++}"
        _executionState.value = PresetExecutionState(
            sessionId = sessionId,
            isExecuting = true,
            activePresetId = preset.id,
        )

        executionJob = scope.launch {
            try {
                val inputText = when (input) {
                    is PresetInput.Text -> input.text
                    else -> {
                        _executionState.update {
                            it.copy(
                                isExecuting = false,
                                error = "This preset input type is not ready on Android yet.",
                            )
                        }
                        return@launch
                    }
                }

                executeTextGraph(
                    sessionId = sessionId,
                    preset = preset,
                    inputText = inputText,
                )

                _executionState.update {
                    it.copy(
                        isExecuting = false,
                        isComplete = true,
                    )
                }
            } catch (e: Exception) {
                _executionState.update {
                    it.copy(
                        isExecuting = false,
                        error = e.message ?: "Execution failed",
                    )
                }
            }
        }
    }

    fun cancelExecution() {
        executionJob?.cancel()
        _executionState.update {
            it.copy(isExecuting = false)
        }
    }

    fun resetState() {
        _executionState.value = PresetExecutionState()
    }

    private fun publishCatalog() {
        _catalogState.value = PresetCatalogState(
            presets = canonicalPresets.map { canonical ->
                val override = storedOverrides.builtInOverrides[canonical.id]
                val resolved = override?.let { canonical.applyOverride(it) } ?: canonical
                val executionCapability = resolveExecutionCapability(resolved)
                ResolvedPreset(
                    preset = resolved,
                    hasOverride = override != null,
                    isBuiltIn = true,
                    executionCapability = executionCapability,
                    placeholderReasons = resolvePlaceholderReasons(
                        preset = resolved,
                        executionCapability = executionCapability,
                    ),
                )
            },
        )
    }

    private fun persistOverrides(updatedOverrides: Map<String, PresetOverride>) {
        storedOverrides = StoredPresetOverrides(
            version = storedOverrides.version,
            builtInOverrides = updatedOverrides,
        )
        overrideStore.save(storedOverrides)
        publishCatalog()
    }

    private suspend fun executeTextGraph(
        sessionId: String,
        preset: Preset,
        inputText: String,
    ) {
        val normalizedEdges = preset.normalizedConnections()
        val incoming = MutableList(preset.blocks.size) { mutableListOf<Int>() }
        val outgoing = MutableList(preset.blocks.size) { mutableListOf<Int>() }
        normalizedEdges.forEach { (from, to) ->
            incoming[to].add(from)
            outgoing[from].add(to)
        }

        val executionOrder = topologicalOrder(
            blockCount = preset.blocks.size,
            edges = normalizedEdges,
        )
        val outputs = mutableMapOf<Int, String>()
        val overlayIndexes = executionOrder.filter { index ->
            val block = preset.blocks[index]
            if (!block.showOverlay) {
                return@filter false
            }
            when (block.blockType) {
                BlockType.TEXT ->
                    block.renderMode in SUPPORTED_MARKDOWN_RENDER_MODES
                BlockType.INPUT_ADAPTER -> true
                else -> false
            }
        }
        val overlayOrder = overlayIndexes.withIndex().associate { it.value to it.index }

        overlayIndexes.forEach { index ->
            val block = preset.blocks[index]
            val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
            val initialText = if (block.blockType == BlockType.INPUT_ADAPTER) inputText else ""
            val initialLoading = block.blockType != BlockType.INPUT_ADAPTER
            _executionState.update {
                it.withWindowState(
                    PresetResultWindowState(
                        id = resultWindowId,
                        blockIdx = index,
                        title = preset.nameEn,
                        markdownText = initialText,
                        isLoading = initialLoading,
                        loadingStatusText = if (initialLoading) loadingStatusText() else null,
                        isStreaming = false,
                        renderMode = if (block.blockType == BlockType.INPUT_ADAPTER) "markdown" else block.renderMode,
                        overlayOrder = overlayOrder.getOrElse(index) { 0 },
                    ),
                )
            }
        }

        executionOrder.forEach { index ->
            val block = preset.blocks[index]
            val shouldSurfaceOverlay = index in overlayOrder
            when (block.blockType) {
                BlockType.INPUT_ADAPTER -> {
                    outputs[index] = inputText
                    if (shouldSurfaceOverlay) {
                        val resultWindowId =
                            PresetResultWindowId(sessionId = sessionId, blockIdx = index)
                        _executionState.update {
                            it.withWindowState(
                                PresetResultWindowState(
                                    id = resultWindowId,
                                    blockIdx = index,
                                    title = preset.nameEn,
                                    markdownText = inputText,
                                    isLoading = false,
                                    loadingStatusText = null,
                                    isStreaming = false,
                                    renderMode = "markdown",
                                    overlayOrder = overlayOrder.getOrElse(index) { 0 },
                                ),
                            )
                        }
                    }
                }
                BlockType.TEXT -> {
                    val sourceText = incoming[index]
                        .mapNotNull(outputs::get)
                        .filter { it.isNotBlank() }
                        .joinToString(separator = "\n\n")
                        .ifBlank { inputText }

                    val blockBuffer = StringBuilder()
                    val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
                    val actualStreamingEnabled = if (block.renderMode == "markdown") {
                        false
                    } else {
                        block.streamingEnabled
                    }
                    val shouldSurfaceStreaming =
                        shouldSurfaceOverlay &&
                            actualStreamingEnabled &&
                            !block.requestsHtmlOutput()
                    val retryChainKind = retryChainKindForBlockType(block.blockType)
                        ?.takeUnless { PresetModelCatalog.isNonLlm(block.model) }
                    var currentModelId = block.model
                    val failedModelIds = mutableListOf<String>()
                    val blockedProviders = linkedSetOf<PresetModelProvider>()
                    val currentApiKeys = apiKeys()
                    val currentRuntimeSettings = runtimeSettings()
                    var result: String? = null
                    while (result == null) {
                        val descriptor = PresetModelCatalog.getById(currentModelId)
                            ?: error("Unknown model config: $currentModelId")

                        val preflight = preflightSkipReason(
                            modelId = currentModelId,
                            provider = descriptor.provider,
                            apiKeys = currentApiKeys,
                            blockedProviders = blockedProviders,
                            settings = currentRuntimeSettings,
                        )
                        if (preflight != null) {
                            if (shouldBlockRetryProvider(preflight)) {
                                blockedProviders += descriptor.provider
                            }
                            failedModelIds += currentModelId
                            val next = resolveNextRetryModel(
                                currentModelId = currentModelId,
                                failedModelIds = failedModelIds,
                                blockedProviders = blockedProviders,
                                chainKind = retryChainKind ?: throw IllegalStateException(preflight),
                                apiKeys = currentApiKeys,
                                settings = currentRuntimeSettings,
                            ) ?: throw IllegalStateException(preflight)
                            currentModelId = next.id
                            if (shouldSurfaceOverlay) {
                                _executionState.update {
                                    it.withWindowState(
                                        PresetResultWindowState(
                                            id = resultWindowId,
                                            blockIdx = index,
                                            title = preset.nameEn,
                                            markdownText = "",
                                            isLoading = true,
                                            loadingStatusText = retryStatusText(next.fullName),
                                            isStreaming = false,
                                            renderMode = block.renderMode,
                                            overlayOrder = overlayOrder.getValue(index),
                                        ),
                                    )
                                }
                            }
                            continue
                        }

                        val attempt = textApiClient.executeStreaming(
                            modelId = currentModelId,
                            prompt = block.resolvePrompt(),
                            inputText = sourceText,
                            apiKeys = currentApiKeys,
                            uiLanguage = uiLanguage(),
                            searchLabel = preset.name(uiLanguage()),
                            streamingEnabled = actualStreamingEnabled,
                            onChunk = { chunk ->
                                if (chunk.startsWith(TextApiClient.WIPE_SIGNAL)) {
                                    blockBuffer.clear()
                                    blockBuffer.append(chunk.removePrefix(TextApiClient.WIPE_SIGNAL))
                                } else {
                                    blockBuffer.append(chunk)
                                }
                                if (shouldSurfaceStreaming) {
                                    _executionState.update {
                                        it.withWindowState(
                                            PresetResultWindowState(
                                                id = resultWindowId,
                                                blockIdx = index,
                                                title = preset.nameEn,
                                                markdownText = blockBuffer.toString(),
                                                isLoading = false,
                                                loadingStatusText = null,
                                                isStreaming = true,
                                                renderMode = block.renderMode,
                                                overlayOrder = overlayOrder.getValue(index),
                                            ),
                                        )
                                    }
                                }
                            },
                        )

                        val error = attempt.exceptionOrNull()
                        if (error != null) {
                            val message = error.message ?: "Execution failed"
                            if (!shouldAdvanceRetryChain(message)) {
                                throw error
                            }
                            if (shouldBlockRetryProvider(message)) {
                                blockedProviders += descriptor.provider
                            }
                            failedModelIds += currentModelId
                            val next = resolveNextRetryModel(
                                currentModelId = currentModelId,
                                failedModelIds = failedModelIds,
                                blockedProviders = blockedProviders,
                                chainKind = retryChainKind ?: throw error,
                                apiKeys = currentApiKeys,
                                settings = currentRuntimeSettings,
                            ) ?: throw error
                            currentModelId = next.id
                            blockBuffer.clear()
                            if (shouldSurfaceOverlay) {
                                _executionState.update {
                                    it.withWindowState(
                                        PresetResultWindowState(
                                            id = resultWindowId,
                                            blockIdx = index,
                                            title = preset.nameEn,
                                            markdownText = "",
                                            isLoading = true,
                                            loadingStatusText = retryStatusText(next.fullName),
                                            isStreaming = false,
                                            renderMode = block.renderMode,
                                            overlayOrder = overlayOrder.getValue(index),
                                        ),
                                    )
                                }
                            }
                            continue
                        }

                        result = attempt.getOrThrow()
                    }

                    val finalResult = requireNotNull(result)
                    outputs[index] = finalResult
                    if (shouldSurfaceOverlay) {
                        _executionState.update {
                            it.withWindowState(
                                PresetResultWindowState(
                                    id = resultWindowId,
                                    blockIdx = index,
                                    title = preset.nameEn,
                                    markdownText = finalResult,
                                    isLoading = false,
                                    loadingStatusText = null,
                                    isStreaming = false,
                                    renderMode = block.renderMode,
                                    overlayOrder = overlayOrder.getValue(index),
                                ),
                            )
                        }
                    }
                }
                else -> error("Non-text block execution is not ready on Android yet.")
            }
        }
    }

    private fun loadingStatusText(): String = when (uiLanguage()) {
        "vi" -> "Đang tải"
        "ko" -> "로딩"
        else -> "Loading"
    }

    private fun retryStatusText(modelName: String): String = when (uiLanguage()) {
        "vi" -> "(Đang thử lại $modelName...)"
        "ko" -> "($modelName 재시도 중...)"
        else -> "(Retrying $modelName...)"
    }

    private fun resolveExecutionCapability(preset: Preset): PresetExecutionCapability {
        if (preset.isMaster || preset.showControllerUi) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.CONTROLLER_MODE_NOT_READY,
            )
        }

        return when (preset.presetType) {
            PresetType.IMAGE -> PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY,
            )
            PresetType.TEXT_SELECT -> PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.TEXT_SELECTION_NOT_READY,
            )
            PresetType.TEXT_INPUT -> resolveTextInputCapability(preset)
            PresetType.MIC, PresetType.DEVICE_AUDIO -> PresetExecutionCapability(
                supported = false,
                reason = if (preset.audioProcessingMode == "realtime") {
                    PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY
                } else {
                    PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY
                },
            )
        }
    }

    private fun resolveTextInputCapability(preset: Preset): PresetExecutionCapability {
        val hasTextBlocks = preset.blocks.any { it.blockType == BlockType.TEXT }
        val hasInputAdapterOverlay = preset.blocks.any {
            it.blockType == BlockType.INPUT_ADAPTER && it.showOverlay
        }
        if (!hasTextBlocks && !hasInputAdapterOverlay) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY,
            )
        }

        val unsupportedTextModel = preset.blocks.firstOrNull { block ->
            block.blockType == BlockType.TEXT && !isTextModelSupported(block.model)
        }
        if (unsupportedTextModel != null) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }

        if (preset.blocks.any { it.blockType !in setOf(BlockType.INPUT_ADAPTER, BlockType.TEXT) }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
            )
        }

        return PresetExecutionCapability(supported = true)
    }

    private fun isTextModelSupported(modelId: String): Boolean {
        val descriptor = PresetModelCatalog.getById(modelId) ?: return false
        if (descriptor.modelType != PresetModelType.TEXT) {
            return false
        }
        return descriptor.provider in setOf(
            PresetModelProvider.GOOGLE,
            PresetModelProvider.CEREBRAS,
            PresetModelProvider.GROQ,
            PresetModelProvider.OPENROUTER,
            PresetModelProvider.GOOGLE_GTX,
            PresetModelProvider.OLLAMA,
        )
    }

    private fun resolvePlaceholderReasons(
        preset: Preset,
        executionCapability: PresetExecutionCapability,
    ): Set<PresetPlaceholderReason> {
        val reasons = linkedSetOf<PresetPlaceholderReason>()

        executionCapability.reason?.let(reasons::add)

        if (preset.autoPaste || preset.autoPasteNewline) {
            reasons += PresetPlaceholderReason.AUTO_PASTE_NOT_READY
        }
        if (preset.hotkeys.isNotEmpty()) {
            reasons += PresetPlaceholderReason.HOTKEYS_NOT_READY
        }
        return reasons
    }
}

private fun Preset.normalizedConnections(): List<Pair<Int, Int>> {
    if (blockConnections.isNotEmpty()) {
        return blockConnections.filter { (from, to) ->
            from in blocks.indices && to in blocks.indices
        }
    }

    if (blocks.size < 2) {
        return emptyList()
    }

    return (0 until blocks.lastIndex).map { index -> index to index + 1 }
}

private fun PresetExecutionState.withWindowState(
    windowState: PresetResultWindowState,
): PresetExecutionState {
    val updated = resultWindows
        .filterNot { it.id == windowState.id }
        .plus(windowState)
        .sortedBy { it.overlayOrder }
    return copy(resultWindows = updated)
}

private val SUPPORTED_MARKDOWN_RENDER_MODES = setOf("markdown", "markdown_stream")

private fun topologicalOrder(
    blockCount: Int,
    edges: List<Pair<Int, Int>>,
): List<Int> {
    val incomingCounts = IntArray(blockCount)
    val outgoing = MutableList(blockCount) { mutableListOf<Int>() }
    edges.forEach { (from, to) ->
        incomingCounts[to] += 1
        outgoing[from] += to
    }

    val ready = ArrayDeque<Int>()
    repeat(blockCount) { index ->
        if (incomingCounts[index] == 0) {
            ready += index
        }
    }

    val ordered = mutableListOf<Int>()
    while (ready.isNotEmpty()) {
        val next = ready.removeFirst()
        ordered += next
        outgoing[next].forEach { child ->
            incomingCounts[child] -= 1
            if (incomingCounts[child] == 0) {
                ready += child
            }
        }
    }

    if (ordered.size != blockCount) {
        error("Preset graph contains an unsupported cycle.")
    }

    return ordered
}

private fun PresetPlaceholderReason.message(): String = when (this) {
    PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY ->
        "Image preset capture is not ready on Android yet."
    PresetPlaceholderReason.TEXT_SELECTION_NOT_READY ->
        "Selected-text capture is not ready on Android yet."
    PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY ->
        "Overlay-style text input is not ready on Android yet."
    PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY ->
        "This preset uses a text model/provider runtime that Android does not implement yet."
    PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY ->
        "Audio capture presets are not ready on Android yet."
    PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY ->
        "Realtime audio presets are not ready on Android yet."
    PresetPlaceholderReason.HTML_RESULT_NOT_READY ->
        "HTML result overlays are not ready on Android yet."
    PresetPlaceholderReason.CONTROLLER_MODE_NOT_READY ->
        "Controller presets are not ready on Android yet."
    PresetPlaceholderReason.AUTO_PASTE_NOT_READY ->
        "Auto-paste integration is not ready on Android yet."
    PresetPlaceholderReason.HOTKEYS_NOT_READY ->
        "Hotkeys are not ready on Android yet."
    PresetPlaceholderReason.GRAPH_EDITING_NOT_READY ->
        "Use the edit button to open the full node graph editor."
    PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY ->
        "Only text-input preset graphs are executable on Android right now."
}

private fun dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock.resolvePrompt(): String {
    var resolved = prompt
    languageVars.forEach { (key, value) ->
        resolved = resolved.replace("{$key}", value)
    }
    return resolved
}

private fun dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock.requestsHtmlOutput(): Boolean {
    val normalizedPrompt = prompt.lowercase()
    return normalizedPrompt.contains("raw html")
        || normalizedPrompt.contains("standalone html")
        || normalizedPrompt.contains("html code")
        || normalizedPrompt.contains("```html")
}
