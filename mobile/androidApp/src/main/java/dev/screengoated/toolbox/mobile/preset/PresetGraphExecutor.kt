package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

internal class PresetGraphExecutor(
    private val textApiClient: TextApiClient,
    private val audioApiClient: AudioApiClient? = null,
    private val visionApiClient: VisionApiClient,
    private val apiKeys: () -> ApiKeys,
    private val runtimeSettings: () -> PresetRuntimeSettings,
    private val uiLanguage: () -> String,
    private val executionState: MutableStateFlow<PresetExecutionState>,
    private val postProcessActions: PresetPostProcessActions = NoOpPostProcessActions,
    private val historyRecorder: dev.screengoated.toolbox.mobile.history.PresetHistoryRecorder =
        dev.screengoated.toolbox.mobile.history.NoOpPresetHistoryRecorder,
) {
    private val audioBlockExecutor by lazy {
        PresetAudioBlockExecutor(
            audioApiClient = requireNotNull(audioApiClient) {
                "AudioApiClient is required before executing AUDIO blocks."
            },
            apiKeys = apiKeys,
            runtimeSettings = runtimeSettings,
            uiLanguage = uiLanguage,
            executionState = executionState,
            historyRecorder = historyRecorder,
        )
    }

    suspend fun executeGraph(
        sessionId: String,
        preset: Preset,
        input: PresetInput,
    ) {
        val inputText = when (input) {
            is PresetInput.Text -> input.text
            else -> ""
        }
        val normalizedEdges = preset.normalizedConnections()
        val incoming = MutableList(preset.blocks.size) { mutableListOf<Int>() }
        normalizedEdges.forEach { (from, to) ->
            incoming[to].add(from)
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
                BlockType.TEXT -> block.renderMode in supportedMarkdownRenderModes
                BlockType.IMAGE -> block.renderMode in supportedMarkdownRenderModes
                BlockType.AUDIO -> block.renderMode in supportedMarkdownRenderModes
                BlockType.INPUT_ADAPTER -> true
            }
        }
        val overlayOrder = overlayIndexes.withIndex().associate { it.value to it.index }

        overlayIndexes.forEach { index ->
            val block = preset.blocks[index]
            val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
            val initialText = if (block.blockType == BlockType.INPUT_ADAPTER) {
                inputAdapterOverlayContent(input, uiLanguage()).orEmpty()
            } else {
                ""
            }
            val initialLoading = block.blockType != BlockType.INPUT_ADAPTER
            executionState.update {
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
            // Isolate per-block errors so one failure doesn't kill all sibling overlays.
            // (Matches Windows: each block's error renders in its own window via per-HWND state.)
            try {
                when (block.blockType) {
                    BlockType.INPUT_ADAPTER -> executeInputAdapterBlock(
                        preset = preset,
                        input = input,
                        inputText = inputText,
                        index = index,
                        overlayOrder = overlayOrder,
                        outputs = outputs,
                        shouldSurfaceOverlay = shouldSurfaceOverlay,
                        sessionId = sessionId,
                    )

                    BlockType.TEXT -> executeTextBlock(
                        preset = preset,
                        inputText = inputText,
                        index = index,
                        incoming = incoming,
                        outputs = outputs,
                        overlayOrder = overlayOrder,
                        shouldSurfaceOverlay = shouldSurfaceOverlay,
                        sessionId = sessionId,
                    )

                    BlockType.IMAGE -> executeImageBlock(
                        preset = preset,
                        imageBytes = (input as? PresetInput.Image)?.pngBytes
                            ?: error("Image bytes required for IMAGE block"),
                        index = index,
                        incoming = incoming,
                        outputs = outputs,
                        overlayOrder = overlayOrder,
                        shouldSurfaceOverlay = shouldSurfaceOverlay,
                        sessionId = sessionId,
                    )

                    BlockType.AUDIO -> audioBlockExecutor.execute(
                        preset = preset,
                        input = (input as? PresetInput.Audio)
                            ?: error("Audio bytes required for AUDIO block"),
                        index = index,
                        incoming = incoming,
                        outputs = outputs,
                        overlayOrder = overlayOrder,
                        shouldSurfaceOverlay = shouldSurfaceOverlay,
                        sessionId = sessionId,
                    )
                }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e // Don't swallow coroutine cancellation
            } catch (e: Exception) {
                // Emit per-window error state; other overlays continue unaffected
                outputs[index] = ""
                if (shouldSurfaceOverlay) {
                    val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
                    executionState.update {
                        it.withWindowState(
                            PresetResultWindowState(
                                id = resultWindowId,
                                blockIdx = index,
                                title = preset.nameEn,
                                markdownText = e.message ?: "Block execution failed",
                                isLoading = false,
                                loadingStatusText = null,
                                isStreaming = false,
                                isError = true,
                                renderMode = block.renderMode,
                                overlayOrder = overlayOrder.getOrElse(index) { 0 },
                            ),
                        )
                    }
                }
            }

            // ── Centralized per-block post-processing (matches Windows step.rs) ──
            val blockOutput = outputs[index] ?: ""
            if (block.autoCopy) {
                when {
                    block.blockType == BlockType.INPUT_ADAPTER && input is PresetInput.Image ->
                        postProcessActions.handleAutoCopyImage(block, input.pngBytes)

                    blockOutput.isNotBlank() ->
                        postProcessActions.handleAutoCopy(block, blockOutput)
                }
            }
            if (blockOutput.isNotBlank()) {
                if (block.autoSpeak) {
                    postProcessActions.handleAutoSpeak(block, blockOutput, index)
                }
            }
        }

        // Auto-paste after ALL blocks complete (matches Windows post_process.rs)
        val shouldSkipFinalAutoPaste = (input as? PresetInput.Audio)?.isStreamingResult == true
        if (preset.autoPaste && !shouldSkipFinalAutoPaste) {
            postProcessActions.handleAutoPaste()
        }
    }

    private fun executeInputAdapterBlock(
        preset: Preset,
        input: PresetInput,
        inputText: String,
        index: Int,
        overlayOrder: Map<Int, Int>,
        outputs: MutableMap<Int, String>,
        shouldSurfaceOverlay: Boolean,
        sessionId: String,
    ) {
        outputs[index] = inputText
        if (input is PresetInput.Audio) {
            historyRecorder.recordAudioResult(
                block = preset.blocks[index],
                wavBytes = input.wavBytes,
                resultText = preset.name(uiLanguage()),
            )
        }
        if (!shouldSurfaceOverlay) {
            return
        }
        val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
        val content = inputAdapterOverlayContent(input, uiLanguage()) ?: return
        executionState.update {
            it.withWindowState(
                PresetResultWindowState(
                    id = resultWindowId,
                    blockIdx = index,
                    title = preset.nameEn,
                    markdownText = content,
                    isLoading = false,
                    loadingStatusText = null,
                    isStreaming = false,
                    renderMode = "markdown",
                    overlayOrder = overlayOrder.getOrElse(index) { 0 },
                ),
            )
        }
    }

    private suspend fun executeTextBlock(
        preset: Preset,
        inputText: String,
        index: Int,
        incoming: List<List<Int>>,
        outputs: MutableMap<Int, String>,
        overlayOrder: Map<Int, Int>,
        shouldSurfaceOverlay: Boolean,
        sessionId: String,
    ) {
        val block = preset.blocks[index]
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
                    emitRetryingWindowState(
                        preset = preset,
                        resultWindowId = resultWindowId,
                        blockIndex = index,
                        overlayOrder = overlayOrder.getValue(index),
                        renderMode = block.renderMode,
                        modelName = next.fullName,
                    )
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
                        executionState.update {
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
                    emitRetryingWindowState(
                        preset = preset,
                        resultWindowId = resultWindowId,
                        blockIndex = index,
                        overlayOrder = overlayOrder.getValue(index),
                        renderMode = block.renderMode,
                        modelName = next.fullName,
                    )
                }
                continue
            }

            result = attempt.getOrThrow()
        }

        val finalResult = requireNotNull(result)
        outputs[index] = finalResult
        historyRecorder.recordTextResult(
            block = block,
            sourceText = sourceText,
            resultText = finalResult,
        )

        if (!shouldSurfaceOverlay) {
            return
        }
        executionState.update {
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

    private suspend fun executeImageBlock(
        preset: Preset,
        imageBytes: ByteArray,
        index: Int,
        incoming: List<List<Int>>,
        outputs: MutableMap<Int, String>,
        overlayOrder: Map<Int, Int>,
        shouldSurfaceOverlay: Boolean,
        sessionId: String,
    ) {
        val block = preset.blocks[index]
        // For chained blocks, previous text output can augment the prompt
        val priorText = incoming[index]
            .mapNotNull(outputs::get)
            .filter { it.isNotBlank() }
            .joinToString(separator = "\n\n")

        val finalPrompt = if (priorText.isNotBlank()) {
            "${block.resolvePrompt()}\n\n$priorText"
        } else {
            block.resolvePrompt()
        }

        val blockBuffer = StringBuilder()
        val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
        val actualStreamingEnabled = if (block.renderMode == "markdown") false else block.streamingEnabled
        val shouldSurfaceStreaming = shouldSurfaceOverlay && actualStreamingEnabled && !block.requestsHtmlOutput()
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
                    emitRetryingWindowState(
                        preset = preset,
                        resultWindowId = resultWindowId,
                        blockIndex = index,
                        overlayOrder = overlayOrder.getValue(index),
                        renderMode = block.renderMode,
                        modelName = next.fullName,
                    )
                }
                continue
            }

            val attempt = visionApiClient.executeStreaming(
                modelId = currentModelId,
                prompt = finalPrompt,
                imageBytes = imageBytes,
                apiKeys = currentApiKeys,
                uiLanguage = uiLanguage(),
                streamingEnabled = actualStreamingEnabled,
                onChunk = { chunk ->
                    if (chunk.startsWith(TextApiClient.WIPE_SIGNAL)) {
                        blockBuffer.clear()
                        blockBuffer.append(chunk.removePrefix(TextApiClient.WIPE_SIGNAL))
                    } else {
                        blockBuffer.append(chunk)
                    }
                    if (shouldSurfaceStreaming) {
                        executionState.update {
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
                if (!shouldAdvanceRetryChain(message)) throw error
                if (shouldBlockRetryProvider(message)) blockedProviders += descriptor.provider
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
                    emitRetryingWindowState(
                        preset = preset,
                        resultWindowId = resultWindowId,
                        blockIndex = index,
                        overlayOrder = overlayOrder.getValue(index),
                        renderMode = block.renderMode,
                        modelName = next.fullName,
                    )
                }
                continue
            }

            result = attempt.getOrThrow()
        }

        val finalResult = requireNotNull(result)
        outputs[index] = finalResult
        historyRecorder.recordImageResult(
            block = block,
            imageBytes = imageBytes,
            resultText = finalResult,
        )

        if (!shouldSurfaceOverlay) return
        executionState.update {
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

    private fun emitRetryingWindowState(
        preset: Preset,
        resultWindowId: PresetResultWindowId,
        blockIndex: Int,
        overlayOrder: Int,
        renderMode: String,
        modelName: String,
    ) {
        executionState.update {
            it.withWindowState(
                PresetResultWindowState(
                    id = resultWindowId,
                    blockIdx = blockIndex,
                    title = preset.nameEn,
                    markdownText = "",
                    isLoading = true,
                    loadingStatusText = retryStatusText(modelName),
                    isStreaming = false,
                    renderMode = renderMode,
                    overlayOrder = overlayOrder,
                ),
            )
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
}
