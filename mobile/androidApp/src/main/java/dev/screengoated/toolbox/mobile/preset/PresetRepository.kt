package dev.screengoated.toolbox.mobile.preset

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
    private val capabilityResolver = PresetExecutionCapabilityResolver()
    private val canonicalPresets = DefaultPresets.all
    private val canonicalById = canonicalPresets.associateBy { it.id }

    private val _catalogState = MutableStateFlow(PresetCatalogState())
    val catalogState: StateFlow<PresetCatalogState> = _catalogState.asStateFlow()

    private val _executionState = MutableStateFlow(PresetExecutionState())
    val executionState: StateFlow<PresetExecutionState> = _executionState.asStateFlow()

    private var storedOverrides = overrideStore.load()
    private var executionJob: Job? = null
    private var nextSessionOrdinal = 1L
    private val graphExecutor = PresetGraphExecutor(
        textApiClient = textApiClient,
        apiKeys = apiKeys,
        runtimeSettings = runtimeSettings,
        uiLanguage = uiLanguage,
        executionState = _executionState,
    )

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

    fun resetAllToDefaults() {
        persistAll(builtInOverrides = emptyMap(), customPresets = emptyMap())
    }

    fun duplicatePreset(id: String, lang: String): String? {
        val source = getResolvedPreset(id)?.preset ?: return null
        val newId = System.currentTimeMillis().toString(16)
        val baseName = source.name(lang)
        var newName = "$baseName Copy"
        var counter = 1
        val existingNames = _catalogState.value.presets.map { it.preset.nameEn }.toSet()
        while (newName in existingNames) {
            newName = "$baseName Copy $counter"
            counter++
        }
        val newPreset = source.copy(
            id = newId,
            nameEn = newName,
            nameVi = newName,
            nameKo = newName,
            isFavorite = false,
        )
        // Store as a full custom preset (not a built-in override)
        val updatedCustom = storedOverrides.customPresets.toMutableMap()
        updatedCustom[newId] = newPreset
        persistAll(
            builtInOverrides = storedOverrides.builtInOverrides,
            customPresets = updatedCustom,
        )
        return newId
    }

    fun deletePreset(id: String) {
        if (canonicalById.containsKey(id)) {
            // Built-in: mark as hidden (will re-appear on reset)
            val updatedOverrides = storedOverrides.builtInOverrides.toMutableMap()
            val existing = updatedOverrides[id] ?: PresetOverride()
            updatedOverrides[id] = existing.copy(isHidden = true)
            persistOverrides(updatedOverrides)
        } else {
            // Custom: remove from custom presets
            val updatedCustom = storedOverrides.customPresets.toMutableMap()
            updatedCustom.remove(id)
            persistAll(
                builtInOverrides = storedOverrides.builtInOverrides,
                customPresets = updatedCustom,
            )
        }
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
        val canonical = canonicalById[id]
        if (canonical != null) {
            // Built-in preset: store as delta override
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
        } else {
            // Custom preset: update the full preset in customPresets
            val existing = storedOverrides.customPresets[id] ?: return
            val updatedPreset = mutation(existing)
            val updatedCustom = storedOverrides.customPresets.toMutableMap()
            updatedCustom[id] = updatedPreset
            persistAll(
                builtInOverrides = storedOverrides.builtInOverrides,
                customPresets = updatedCustom,
            )
        }
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

                graphExecutor.executeTextGraph(
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

    fun refineInPlace(
        preset: Preset,
        previousText: String,
        refinePrompt: String,
        onChunk: (String) -> Unit,
        onComplete: (Result<String>) -> Unit,
    ): Job {
        val blockModel = preset.blocks.firstOrNull()?.model.orEmpty()
        val modelId = blockModel.ifEmpty {
            runtimeSettings().modelPriorityChains.textToText.firstOrNull()
        } ?: return scope.launch { onComplete(Result.failure(Exception("No model configured"))) }
        val keys = apiKeys()
        val lang = uiLanguage()

        val mainHandler = android.os.Handler(android.os.Looper.getMainLooper())
        return scope.launch(Dispatchers.IO) {
            val result = textApiClient.executeStreaming(
                modelId = modelId,
                prompt = "Content:\n$previousText\n\nInstruction:\n$refinePrompt\n\nOutput ONLY the result.",
                inputText = previousText,
                apiKeys = keys,
                uiLanguage = lang,
                searchLabel = null,
                onChunk = { chunk -> mainHandler.post { onChunk(chunk) } },
                streamingEnabled = true,
            )
            kotlinx.coroutines.withContext(Dispatchers.Main) {
                onComplete(result)
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
        val builtInResolved = canonicalPresets.mapNotNull { canonical ->
            val override = storedOverrides.builtInOverrides[canonical.id]
            if (override?.isHidden == true) return@mapNotNull null
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
        }
        val customResolved = storedOverrides.customPresets.values.map { preset ->
            val executionCapability = resolveExecutionCapability(preset)
            ResolvedPreset(
                preset = preset,
                hasOverride = false,
                isBuiltIn = false,
                executionCapability = executionCapability,
                placeholderReasons = resolvePlaceholderReasons(
                    preset = preset,
                    executionCapability = executionCapability,
                ),
            )
        }
        _catalogState.value = PresetCatalogState(
            presets = builtInResolved + customResolved,
        )
    }

    private fun persistOverrides(updatedOverrides: Map<String, PresetOverride>) {
        persistAll(builtInOverrides = updatedOverrides, customPresets = storedOverrides.customPresets)
    }

    private fun persistAll(
        builtInOverrides: Map<String, PresetOverride>,
        customPresets: Map<String, Preset>,
    ) {
        storedOverrides = StoredPresetOverrides(
            version = storedOverrides.version,
            builtInOverrides = builtInOverrides,
            customPresets = customPresets,
        )
        overrideStore.save(storedOverrides)
        publishCatalog()
    }

    private fun resolveExecutionCapability(preset: Preset): PresetExecutionCapability {
        return capabilityResolver.resolveExecutionCapability(preset)
    }

    private fun resolvePlaceholderReasons(
        preset: Preset,
        executionCapability: PresetExecutionCapability,
    ): Set<PresetPlaceholderReason> {
        return capabilityResolver.resolvePlaceholderReasons(
            preset = preset,
            executionCapability = executionCapability,
        )
    }
}
