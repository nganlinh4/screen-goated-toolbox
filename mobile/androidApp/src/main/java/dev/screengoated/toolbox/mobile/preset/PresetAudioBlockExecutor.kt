package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import kotlinx.coroutines.flow.MutableStateFlow

internal class PresetAudioBlockExecutor(
    private val audioApiClient: AudioApiClient,
    private val apiKeys: () -> ApiKeys,
    private val runtimeSettings: () -> PresetRuntimeSettings,
    private val uiLanguage: () -> String,
    private val executionState: MutableStateFlow<PresetExecutionState>,
    private val historyRecorder: dev.screengoated.toolbox.mobile.history.PresetHistoryRecorder,
) {
    suspend fun execute(
        preset: Preset,
        input: PresetInput.Audio,
        index: Int,
        incoming: List<List<Int>>,
        outputs: MutableMap<Int, String>,
        overlayOrder: Map<Int, Int>,
        shouldSurfaceOverlay: Boolean,
        sessionId: String,
    ) {
        val block = preset.blocks[index]
        val priorText = incoming[index]
            .mapNotNull(outputs::get)
            .filter { it.isNotBlank() }
            .joinToString(separator = "\n\n")
        val finalPrompt = listOf(block.resolvePrompt(), priorText)
            .filter { it.isNotBlank() }
            .joinToString(separator = "\n\n")

        val blockBuffer = StringBuilder()
        val resultWindowId = PresetResultWindowId(sessionId = sessionId, blockIdx = index)
        val actualStreamingEnabled = if (block.renderMode == "markdown") false else block.streamingEnabled
        val shouldSurfaceStreaming = shouldSurfaceOverlay && actualStreamingEnabled && !block.requestsHtmlOutput()
        val descriptor = PresetModelCatalog.getById(block.model)
            ?: error("Unknown model config: ${block.model}")
        preflightSkipReason(
            modelId = block.model,
            provider = descriptor.provider,
            apiKeys = apiKeys(),
            blockedProviders = emptySet(),
            settings = runtimeSettings(),
        )?.let { reason ->
            throw IllegalStateException(reason)
        }

        val result = input.precomputedTranscript
            ?.takeIf { incoming[index].isEmpty() }
            ?: audioApiClient.executeStreaming(
                modelId = block.model,
                prompt = finalPrompt,
                wavBytes = input.wavBytes,
                apiKeys = apiKeys(),
                uiLanguage = uiLanguage(),
                streamingEnabled = actualStreamingEnabled,
                onChunk = { chunk ->
                    blockBuffer.append(chunk)
                    if (shouldSurfaceStreaming) {
                        executionState.value = executionState.value.withWindowState(
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
                },
            ).getOrThrow()

        outputs[index] = result
        historyRecorder.recordAudioResult(
            block = block,
            wavBytes = input.wavBytes,
            resultText = result,
        )

        if (!shouldSurfaceOverlay) {
            return
        }
        executionState.value = executionState.value.withWindowState(
            PresetResultWindowState(
                id = resultWindowId,
                blockIdx = index,
                title = preset.nameEn,
                markdownText = result,
                isLoading = false,
                loadingStatusText = null,
                isStreaming = input.isStreamingResult,
                renderMode = block.renderMode,
                overlayOrder = overlayOrder.getValue(index),
            ),
        )
    }
}
