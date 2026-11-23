package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.CancellationException
import java.io.IOException

internal class PresetAudioBlockExecutor(
    private val audioApiClient: AudioApiClient,
    private val apiKeys: () -> ApiKeys,
    private val runtimeSettings: () -> PresetRuntimeSettings,
    private val uiLanguage: () -> String,
    private val executionState: MutableStateFlow<PresetExecutionState>,
    private val sessionOwnership: PresetSessionOwnership,
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
        val shouldSurfaceStreaming = shouldSurfaceOverlay && actualStreamingEnabled
        val descriptor = PresetModelCatalog.getById(block.model)
            ?: error("Unknown model config: ${block.model}")
        require(descriptor.modelType == PresetModelType.AUDIO) { "Incompatible model: ${block.model}" }
        val modelAttempt = PresetModelAttempt(block.model)
        sessionOwnership.owned(sessionId) { (preflightSkipReason(
            modelId = block.model,
            provider = descriptor.provider,
            apiKeys = apiKeys(),
            blockedProviders = emptySet(),
            settings = runtimeSettings(),
        ) ?: claimPresetModelAttempt(modelAttempt))?.let { reason ->
            throw IllegalStateException(reason)
        } }

        val result = try {
            preset.precomputedAudioForBlock(index, input.precomputedTranscript)
                ?: audioApiClient.executeStreaming(
                    modelId = block.model,
                    prompt = finalPrompt,
                    wavBytes = input.wavBytes,
                    apiKeys = apiKeys(),
                    uiLanguage = uiLanguage(),
                    streamingEnabled = actualStreamingEnabled,
                    inputLanguage = block.languageVars["input_language"],
                    onChunk = { chunk ->
                        blockBuffer.append(chunk)
                        if (shouldSurfaceStreaming) {
                            executionState.update { it.withWindowState(
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
                                    modelId = descriptor.id,
                                    modelProvider = descriptor.provider,
                                ),
                            ) }
                        }
                    },
                ).getOrThrow()
        } catch (cancelled: CancellationException) {
            releasePresetModelProbe(modelAttempt)
            throw cancelled
        } catch (error: Exception) {
            try { sessionOwnership.ensureActive(sessionId) } catch (cancelled: CancellationException) {
                releasePresetModelProbe(modelAttempt)
                throw cancelled
            }
            sessionOwnership.ownedProbe(sessionId, modelAttempt) { recordPresetModelFailure(modelAttempt, error.message.orEmpty()) }
            throw error
        }

        try { sessionOwnership.ensureActive(sessionId) } catch (cancelled: CancellationException) {
            releasePresetModelProbe(modelAttempt)
            throw cancelled
        }
        if (result.isBlank()) {
            sessionOwnership.ownedProbe(sessionId, modelAttempt) { recordPresetModelFailure(modelAttempt, "Audio transcription returned no text.") }
            throw IOException("Audio transcription returned no text.")
        }
        sessionOwnership.ownedProbe(sessionId, modelAttempt) { recordPresetModelSuccess(modelAttempt) }
        outputs[index] = result
        sessionOwnership.owned(sessionId) { historyRecorder.recordAudioResult(
            block = block,
            wavBytes = input.wavBytes,
            resultText = result,
        ) }

        if (!shouldSurfaceOverlay) {
            return
        }
        executionState.update { it.withWindowState(
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
                modelId = descriptor.id,
                modelProvider = descriptor.provider,
            ),
        ) }
    }
}
