package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockResult
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
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
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val _executionState = MutableStateFlow(PresetExecutionState())
    val executionState: StateFlow<PresetExecutionState> = _executionState.asStateFlow()

    private var executionJob: Job? = null

    fun getAllPresets(): List<Preset> = DefaultPresets.all

    fun getPresetsByType(type: PresetType): List<Preset> = when (type) {
        PresetType.IMAGE -> DefaultPresets.imagePresets
        PresetType.TEXT_SELECT -> DefaultPresets.textSelectPresets
        PresetType.TEXT_INPUT -> DefaultPresets.textInputPresets
        PresetType.MIC -> DefaultPresets.micPresets
        PresetType.DEVICE_AUDIO -> DefaultPresets.deviceAudioPresets
    }

    fun executePreset(preset: Preset, input: PresetInput) {
        executionJob?.cancel()
        _executionState.value = PresetExecutionState(
            isExecuting = true,
            activePreset = preset,
        )

        executionJob = scope.launch {
            try {
                // Find the first non-input-adapter text block
                val textBlock = preset.blocks.firstOrNull {
                    it.blockType == BlockType.TEXT && it.prompt.isNotBlank()
                }

                if (textBlock == null) {
                    _executionState.update {
                        it.copy(isExecuting = false, error = "No processing block found")
                    }
                    return@launch
                }

                // Substitute language variables in prompt
                var resolvedPrompt = textBlock.prompt
                for ((key, value) in textBlock.languageVars) {
                    resolvedPrompt = resolvedPrompt.replace("{$key}", value)
                }

                // Build the full prompt with input
                val inputText = when (input) {
                    is PresetInput.Text -> input.text
                    else -> ""
                }

                val fullPrompt = if (inputText.isNotBlank() && resolvedPrompt.isNotBlank()) {
                    "$resolvedPrompt\n\n$inputText"
                } else if (inputText.isNotBlank()) {
                    inputText
                } else {
                    resolvedPrompt
                }

                // Execute API call with streaming
                val result = textApiClient.executeStreaming(
                    model = textBlock.model,
                    prompt = fullPrompt,
                    inputText = "",
                    apiKeys = apiKeys(),
                    onChunk = { chunk ->
                        _executionState.update {
                            it.copy(streamingText = it.streamingText + chunk)
                        }
                    },
                )

                result.fold(
                    onSuccess = { finalText ->
                        _executionState.update {
                            it.copy(
                                isExecuting = false,
                                isComplete = true,
                                streamingText = finalText,
                                blockResults = listOf(
                                    BlockResult(0, finalText, textBlock.model),
                                ),
                            )
                        }
                    },
                    onFailure = { e ->
                        _executionState.update {
                            it.copy(
                                isExecuting = false,
                                error = e.message ?: "Execution failed",
                            )
                        }
                    },
                )
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
}
