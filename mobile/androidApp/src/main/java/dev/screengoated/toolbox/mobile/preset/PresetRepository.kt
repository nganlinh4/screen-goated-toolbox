package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockResult
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

data class PresetExecutionState(
    val isExecuting: Boolean = false,
    val activePreset: Preset? = null,
    val streamingText: String = "",
    val blockResults: List<BlockResult> = emptyList(),
    val error: String? = null,
    val isComplete: Boolean = false,
)

class PresetRepository(
    private val textApiClient: TextApiClient,
    private val apiKeys: () -> ApiKeys,
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
                activePreset = preset,
                error = capability.reason?.message() ?: "Preset execution is not ready on Android yet.",
            )
            return
        }

        executionJob?.cancel()
        _executionState.value = PresetExecutionState(
            isExecuting = true,
            activePreset = preset,
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

                val blockResults = executeTextGraph(
                    preset = preset,
                    inputText = inputText,
                )
                val terminalText = resolveTerminalText(
                    preset = preset,
                    blockResults = blockResults,
                )

                _executionState.update {
                    it.copy(
                        isExecuting = false,
                        isComplete = true,
                        streamingText = terminalText,
                        blockResults = blockResults,
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
        preset: Preset,
        inputText: String,
    ): List<BlockResult> {
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
        val blockResults = mutableListOf<BlockResult>()
        val textBlockCount = preset.blocks.count { it.blockType == BlockType.TEXT }
        val canStreamToUi = textBlockCount == 1

        executionOrder.forEach { index ->
            val block = preset.blocks[index]
            when (block.blockType) {
                BlockType.INPUT_ADAPTER -> outputs[index] = inputText
                BlockType.TEXT -> {
                    val sourceText = incoming[index]
                        .mapNotNull(outputs::get)
                        .filter { it.isNotBlank() }
                        .joinToString(separator = "\n\n")
                        .ifBlank { inputText }

                    val blockBuffer = StringBuilder()
                    val result = textApiClient.executeStreaming(
                        model = block.model,
                        prompt = block.resolvePrompt(),
                        inputText = sourceText,
                        apiKeys = apiKeys(),
                        onChunk = { chunk ->
                            blockBuffer.append(chunk)
                            if (canStreamToUi) {
                                _executionState.update {
                                    it.copy(streamingText = blockBuffer.toString())
                                }
                            }
                        },
                    ).getOrThrow()

                    outputs[index] = result
                    blockResults += BlockResult(
                        blockIdx = index,
                        text = result,
                        model = block.model,
                    )
                }
                else -> error("Non-text block execution is not ready on Android yet.")
            }
        }

        return blockResults
    }

    private fun resolveTerminalText(
        preset: Preset,
        blockResults: List<BlockResult>,
    ): String {
        val terminalIndexes = preset.terminalTextBlockIndexes()
        return blockResults
            .filter { it.blockIdx in terminalIndexes }
            .joinToString(separator = "\n\n") { it.text }
            .ifBlank { blockResults.lastOrNull()?.text.orEmpty() }
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
        if (preset.blocks.none { it.blockType == BlockType.TEXT }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY,
            )
        }

        if (preset.blocks.any { it.blockType !in setOf(BlockType.INPUT_ADAPTER, BlockType.TEXT) }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
            )
        }

        if (preset.blocks.any { it.requestsHtmlOutput() }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.HTML_RESULT_NOT_READY,
            )
        }

        return PresetExecutionCapability(supported = true)
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
        if (preset.blocks.isNotEmpty()) {
            reasons += PresetPlaceholderReason.GRAPH_EDITING_NOT_READY
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

private fun Preset.terminalTextBlockIndexes(): Set<Int> {
    val outgoing = normalizedConnections().groupBy({ it.first }, { it.second })
    return blocks.indices
        .filter { index ->
            blocks[index].blockType == BlockType.TEXT &&
                outgoing[index].isNullOrEmpty()
        }
        .toSet()
}

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
        "Graph editing is still a placeholder on Android."
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
