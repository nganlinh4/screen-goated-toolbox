package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.CancellationException
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText

internal class PresetGraphExecutor(
    private val textApiClient: TextApiClient,
    private val audioApiClient: AudioApiClient? = null,
    private val visionApiClient: VisionApiClient,
    private val apiKeys: () -> ApiKeys,
    private val runtimeSettings: () -> PresetRuntimeSettings,
    private val uiLanguage: () -> String,
    private val executionState: MutableStateFlow<PresetExecutionState>,
    private val sessionOwnership: PresetSessionOwnership = PresetSessionOwnership(executionState),
    private val toastBus: AppToastBus,
    private val postProcessActions: () -> PresetPostProcessActions = { NoOpPostProcessActions },
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
            sessionOwnership = sessionOwnership,
            historyRecorder = historyRecorder,
        )
    }

    suspend fun executeGraph(
        sessionId: String,
        preset: Preset,
        input: PresetInput,
    ) {
        sessionOwnership.ensureActive(sessionId)
        val inputText = when (input) {
            is PresetInput.Text -> input.text
            else -> ""
        }
        val normalizedEdges = preset.normalizedConnections()
        val incoming = MutableList(preset.blocks.size) { mutableListOf<Int>() }
        normalizedEdges.forEach { (from, to) ->
            incoming[to].add(from)
        }

        val executionOrder = preset.reachableExecutionOrder(normalizedEdges)
        val outputs: MutableMap<Int, String> = java.util.concurrent.ConcurrentHashMap()
        val running = executionOrder.toSet()
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
            val initialModel = block.model
                .takeUnless { block.blockType == BlockType.INPUT_ADAPTER }
                ?.let(PresetModelCatalog::getById)
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
                        loadingStatusText = null,
                        isStreaming = false,
                        renderMode = if (block.blockType == BlockType.INPUT_ADAPTER) "markdown" else block.renderMode,
                        overlayOrder = overlayOrder.getOrElse(index) { 0 },
                        modelId = initialModel?.id.orEmpty(),
                        modelProvider = initialModel?.provider,
                    ).let { state ->
                        if (block.blockType == BlockType.INPUT_ADAPTER) state.withInputAdapterMedia(input, initialText) else state
                    },
                )
            }
        }

        runPresetDataflow(executionOrder, incoming) { index ->
            sessionOwnership.ensureActive(sessionId)
            val block = preset.blocks[index]
            val shouldSurfaceOverlay = index in overlayOrder
            if (shouldSkipDownstreamBlock(incoming[index], running, outputs) { preset.blocks[it].blockType == BlockType.INPUT_ADAPTER }) {
                outputs[index] = ""
                executionState.update { it.withoutWindowState(PresetResultWindowId(sessionId = sessionId, blockIdx = index)) }
                return@runPresetDataflow
            }
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
                sessionOwnership.ensureActive(sessionId)
                val rawError = e.message ?: e.toString()
                val apiKeyNotice = apiKeyErrorToastText(rawError, uiLanguage())
                val visibleError = apiKeyNotice ?: hiddenBlockErrorText(rawError, uiLanguage())
                if (!shouldSurfaceOverlay || apiKeyNotice != null) {
                    sessionOwnership.owned(sessionId) { toastBus.show(visibleError) }
                }
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
            sessionOwnership.ensureActive(sessionId)
            sessionOwnership.owned(sessionId) {
                val blockOutput = outputs[index] ?: ""
                if (block.autoCopy) {
                    when {
                        block.blockType == BlockType.INPUT_ADAPTER && input is PresetInput.Image ->
                            postProcessActions().handleAutoCopyImage(block, input.pngBytes, sessionOwnership.effectLease(sessionId))

                        blockOutput.isNotBlank() ->
                            postProcessActions().handleAutoCopy(block, blockOutput, sessionOwnership.effectLease(sessionId))
                    }
                }
                if (blockOutput.isNotBlank()) {
                    if (block.autoSpeak) {
                        postProcessActions().handleAutoSpeak(block, blockOutput, index, sessionOwnership.effectLease(sessionId))
                    }
                }
            }
        }

        // Auto-paste after ALL blocks complete (matches Windows post_process.rs)
        val shouldSkipFinalAutoPaste = (input as? PresetInput.Audio)?.isStreamingResult == true
        if (preset.autoPaste && !shouldSkipFinalAutoPaste) {
            sessionOwnership.owned(sessionId) { postProcessActions().handleAutoPaste(sessionOwnership.effectLease(sessionId)) }
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
            sessionOwnership.owned(sessionId) { historyRecorder.recordAudioResult(
                block = preset.blocks[index],
                wavBytes = input.wavBytes,
                resultText = preset.name(uiLanguage()),
            ) }
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
                ).withInputAdapterMedia(input, content),
            )
        }
    }

    private suspend fun executeStreamingBlock(
        preset: Preset,
        index: Int,
        outputs: MutableMap<Int, String>,
        overlayOrder: Map<Int, Int>,
        shouldSurfaceOverlay: Boolean,
        sessionId: String,
        recordResult: (finalResult: String) -> Unit,
        attempt: suspend (modelId: String, apiKeys: ApiKeys, onChunk: (String) -> Unit) -> Result<String>,
    ) {
        val block = preset.blocks[index]
        val blockBuffer = StringBuilder()
        val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
        val shouldSurfaceStreaming =
            shouldSurfaceOverlay &&
                block.surfaceStreamingEnabled()
        val failedModelIds = mutableListOf<String>()
        val blockedProviders = linkedSetOf<PresetModelProvider>()
        val currentApiKeys = apiKeys()
        val currentRuntimeSettings = runtimeSettings()
        val retryOrder = retryChainKindForBlockType(block.blockType)
            ?.takeUnless { PresetModelCatalog.isNonLlm(block.model) }
            ?.let { PresetAttemptOrder(it, block.model, block.searchEnabled, currentApiKeys, currentRuntimeSettings) }
        var currentModelId = retryOrder?.initialId() ?: block.model
        var result: String? = null

        while (result == null) {
            sessionOwnership.ensureActive(sessionId)
            val descriptor = PresetModelCatalog.getById(currentModelId)
                ?: error("Unknown model config: $currentModelId")
            val expectedType = when (block.blockType) {
                BlockType.IMAGE -> PresetModelType.VISION
                BlockType.TEXT -> PresetModelType.TEXT
                else -> error("Unsupported block type: ${block.blockType}")
            }
            require(descriptor.modelType == expectedType) { "Incompatible model: $currentModelId" }
            requireExplicitSearchOptIn(descriptor, block.searchEnabled)

            val modelAttempt = PresetModelAttempt(currentModelId)
            val preflight = sessionOwnership.owned(sessionId) { preflightSkipReason(
                modelId = currentModelId,
                provider = descriptor.provider,
                apiKeys = currentApiKeys,
                blockedProviders = blockedProviders,
                settings = currentRuntimeSettings,
            ) ?: claimPresetModelAttempt(modelAttempt) }
            if (preflight != null) {
                if (shouldBlockRetryProvider(preflight)) {
                    blockedProviders += descriptor.provider
                }
                failedModelIds += currentModelId
                val next = retryOrder?.next(currentModelId, failedModelIds, blockedProviders)
                    ?: throw IllegalStateException(preflight)
                currentModelId = next.id
                if (shouldSurfaceOverlay) {
                    emitRetryingWindowState(
                        preset = preset,
                        resultWindowId = resultWindowId,
                        blockIndex = index,
                        overlayOrder = overlayOrder.getValue(index),
                        renderMode = block.renderMode,
                        model = next,
                    )
                }
                continue
            }

            if (failedModelIds.isEmpty() && currentModelId != block.model && shouldSurfaceOverlay) {
                executionState.update { state ->
                    val window = state.resultWindows.firstOrNull { it.id == resultWindowId }
                    window?.let {
                        state.withWindowState(it.copy(modelId = descriptor.id, modelProvider = descriptor.provider))
                    } ?: state
                }
            }
            val attemptResult = try { attempt(currentModelId, currentApiKeys) { chunk ->
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
                                modelId = currentModelId,
                                modelProvider = descriptor.provider,
                            ),
                        )
                    }
                }
            } } catch (e: Exception) { releasePresetModelProbe(modelAttempt); throw e }

            try { sessionOwnership.ensureActive(sessionId) } catch (e: CancellationException) {
                releasePresetModelProbe(modelAttempt)
                throw e
            }

            val completedAttempt = attemptResult.fold(
                onSuccess = { value ->
                    if (value.isBlank()) {
                        Result.failure(IllegalStateException("EMPTY_MODEL_RESPONSE:${descriptor.fullName}"))
                    } else {
                        Result.success(value)
                    }
                },
                onFailure = Result.Companion::failure,
            )

            val error = completedAttempt.exceptionOrNull()
            if (error != null) {
                if (error is CancellationException) {
                    releasePresetModelProbe(modelAttempt)
                    throw error
                }
                val message = error.message ?: "Execution failed"
                sessionOwnership.ownedProbe(sessionId, modelAttempt) { recordPresetModelFailure(modelAttempt, message) }
                if (!shouldAdvanceRetryChain(message)) {
                    throw error
                }
                if (shouldBlockRetryProvider(message)) {
                    blockedProviders += descriptor.provider
                }
                failedModelIds += currentModelId
                val next = retryOrder?.next(currentModelId, failedModelIds, blockedProviders) ?: throw error
                currentModelId = next.id
                blockBuffer.clear()
                if (shouldSurfaceOverlay) {
                    emitRetryingWindowState(
                        preset = preset,
                        resultWindowId = resultWindowId,
                        blockIndex = index,
                        overlayOrder = overlayOrder.getValue(index),
                        renderMode = block.renderMode,
                        model = next,
                    )
                }
                continue
            }

            sessionOwnership.ownedProbe(sessionId, modelAttempt) { recordPresetModelSuccess(modelAttempt) }
            result = completedAttempt.getOrThrow()
        }

        val finalResult = requireNotNull(result)
        outputs[index] = finalResult
        sessionOwnership.owned(sessionId) { recordResult(finalResult) }

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
                    modelId = currentModelId,
                    modelProvider = PresetModelCatalog.getById(currentModelId)?.provider,
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

        executeStreamingBlock(
            preset = preset,
            index = index,
            outputs = outputs,
            overlayOrder = overlayOrder,
            shouldSurfaceOverlay = shouldSurfaceOverlay,
            sessionId = sessionId,
            recordResult = { finalResult ->
                historyRecorder.recordTextResult(
                    block = block,
                    sourceText = sourceText,
                    resultText = finalResult,
                )
            },
            attempt = { modelId, attemptApiKeys, onChunk ->
                textApiClient.executeStreaming(
                    modelId = modelId,
                    prompt = block.resolvePrompt(),
                    inputText = sourceText,
                    apiKeys = attemptApiKeys,
                    uiLanguage = uiLanguage(),
                    searchLabel = preset.name(uiLanguage()),
                    streamingEnabled = requireNotNull(PresetModelCatalog.getById(modelId))
                        .transportStreamingEnabled(),
                    searchEnabled = block.searchEnabled,
                    targetLanguage = block.gtxTargetLanguage(),
                    onChunk = onChunk,
                )
            },
        )
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
        require(!block.searchEnabled) { "SEARCH_UNSUPPORTED:image" }
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

        executeStreamingBlock(
            preset = preset,
            index = index,
            outputs = outputs,
            overlayOrder = overlayOrder,
            shouldSurfaceOverlay = shouldSurfaceOverlay,
            sessionId = sessionId,
            recordResult = { finalResult ->
                historyRecorder.recordImageResult(
                    block = block,
                    imageBytes = imageBytes,
                    resultText = finalResult,
                )
            },
            attempt = { modelId, attemptApiKeys, onChunk ->
                visionApiClient.executeStreaming(
                    modelId = modelId,
                    prompt = finalPrompt,
                    imageBytes = imageBytes,
                    apiKeys = attemptApiKeys,
                    uiLanguage = uiLanguage(),
                    streamingEnabled = requireNotNull(PresetModelCatalog.getById(modelId))
                        .transportStreamingEnabled(),
                    onChunk = onChunk,
                )
            },
        )
    }

    private fun emitRetryingWindowState(
        preset: Preset,
        resultWindowId: PresetResultWindowId,
        blockIndex: Int,
        overlayOrder: Int,
        renderMode: String,
        model: PresetModelDescriptor,
    ) {
        executionState.update {
            it.withWindowState(
                PresetResultWindowState(
                    id = resultWindowId,
                    blockIdx = blockIndex,
                    title = preset.nameEn,
                    markdownText = "",
                    isLoading = true,
                    loadingStatusText = retryStatusText(model.fullName),
                    isStreaming = false,
                    renderMode = renderMode,
                    overlayOrder = overlayOrder,
                    modelId = model.id,
                    modelProvider = model.provider,
                ),
            )
        }
    }

    private fun retryStatusText(modelName: String): String = when (uiLanguage()) {
        "vi" -> "(Đang thử lại $modelName...)"
        "ko" -> "($modelName 재시도 중...)"
        else -> "(Retrying $modelName...)"
    }

    private fun hiddenBlockErrorText(raw: String, lang: String): String = when (lang) {
        "vi" -> "Không thể chạy tác vụ ẩn: $raw"
        "ko" -> "숨겨진 작업을 실행할 수 없습니다: $raw"
        else -> "Hidden preset step failed: $raw"
    }
}
