package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_IMAGE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_TEXT_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.PRESET_AUDIO_TRANSCRIBE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.AppToastBus
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class PresetRepository(
    private val textApiClient: TextApiClient,
    private val audioApiClient: AudioApiClient? = null,
    private val visionApiClient: VisionApiClient,
    private val apiKeys: () -> ApiKeys,
    private val runtimeSettings: () -> PresetRuntimeSettings,
    private val uiLanguage: () -> String,
    private val overrideStore: PresetOverrideStore,
    private val toastBus: AppToastBus,
    private val historyRecorder: dev.screengoated.toolbox.mobile.history.PresetHistoryRecorder =
        dev.screengoated.toolbox.mobile.history.NoOpPresetHistoryRecorder,
    private val mainDispatcher: CoroutineDispatcher = Dispatchers.Main,
    private val newPresetId: () -> String = { java.util.UUID.randomUUID().toString() },
) {
    private val scope = CoroutineScope(SupervisorJob() + mainDispatcher)
    private val capabilityResolver = PresetExecutionCapabilityResolver()
    private val canonicalPresets = DefaultPresets.all
    private val canonicalById = canonicalPresets.associateBy { it.id }

    private val _catalogState = MutableStateFlow(PresetCatalogState())
    val catalogState: StateFlow<PresetCatalogState> = _catalogState.asStateFlow()

    private val _executionState = MutableStateFlow(PresetExecutionState())
    val executionState: StateFlow<PresetExecutionState> = _executionState.asStateFlow()
    private val sessionOwnership = PresetSessionOwnership(_executionState)

    private val initialOverrides = try {
        Result.success(overrideStore.load())
    } catch (error: Exception) {
        Result.failure(error)
    }
    private var storedOverrides = initialOverrides.getOrDefault(StoredPresetOverrides())
    private var persistenceFailure: PresetPersistenceFailure? =
        if (initialOverrides.isFailure) PresetPersistenceFailure.READ else null
    private var executionJob: Job? = null
    private var nextSessionOrdinal = 1L
    private val rowIncarnations = mutableMapOf<String, Long>()
    private var nextRowIncarnation = 0L
    private val rowEpoch = java.util.UUID.randomUUID().toString()
    // Read per execution: the overlay controller that owns effects can be recreated.
    var postProcessActions: PresetPostProcessActions = NoOpPostProcessActions

    fun releasePostProcessActions(owner: PresetPostProcessActions) {
        if (postProcessActions === owner) postProcessActions = NoOpPostProcessActions
    }

    private val graphExecutor by lazy {
        PresetGraphExecutor(
            textApiClient = textApiClient,
            audioApiClient = audioApiClient,
            visionApiClient = visionApiClient,
            apiKeys = apiKeys,
            runtimeSettings = runtimeSettings,
            uiLanguage = uiLanguage,
            executionState = _executionState,
            sessionOwnership = sessionOwnership,
            toastBus = toastBus,
            postProcessActions = { postProcessActions },
            historyRecorder = historyRecorder,
        )
    }

    init {
        publishCatalog()
        scope.launch {
            combine(PresetModelFeed.state, PresetCustomModelRegistry.revision) { _, _ -> Unit }
                .collect { publishCatalog() }
        }
    }

    fun getAllPresets(): List<Preset> = _catalogState.value.presets.map { it.preset }

    @Synchronized
    fun getResolvedPreset(id: String): ResolvedPreset? = unambiguousVisibleRow(id)?.let {
        val capability = resolveExecutionCapability(it.preset)
        it.copy(
            executionCapability = capability,
            placeholderReasons = resolvePlaceholderReasons(it.preset, capability),
        )
    }

    private fun unambiguousVisibleRow(id: String): ResolvedPreset? {
        val builtInCount = if (canonicalById.containsKey(id)) 1 else 0
        if (builtInCount + storedOverrides.customPresets.values.count { it.id == id } != 1) return null
        return _catalogState.value.findPreset(id)
    }

    @Synchronized
    fun toggleFavorite(id: String) {
        updateBuiltInOverride(id) { it.copy(isFavorite = !it.isFavorite) }
    }

    @Synchronized
    internal fun toggleFavoriteRow(rowKey: String) {
        updatePresetRow(rowKey) { it.copy(isFavorite = !it.isFavorite) }
    }

    @Synchronized
    fun resetAllToDefaults() {
        persistAll(builtInOverrides = emptyMap(), customPresets = emptyMap())
    }

    @Synchronized
    fun createCustomPreset(type: PresetType, lang: String): String? {
        val newId = allocatePresetId() ?: return null
        val count = _catalogState.value.presets.size + 1
        val typeName = when (type) {
            PresetType.IMAGE -> when (lang) { "vi" -> "Ảnh"; "ko" -> "이미지"; else -> "Image" }
            PresetType.TEXT_SELECT -> when (lang) { "vi" -> "Bôi text"; "ko" -> "텍스트 선택"; else -> "Text Select" }
            PresetType.TEXT_INPUT -> when (lang) { "vi" -> "Nhập text"; "ko" -> "텍스트 입력"; else -> "Text Input" }
            PresetType.MIC -> when (lang) { "vi" -> "Thu micro"; "ko" -> "마이크"; else -> "Mic" }
            PresetType.DEVICE_AUDIO -> when (lang) { "vi" -> "Âm thanh máy"; "ko" -> "시스템 오디오"; else -> "Device Audio" }
        }
        val name = "$typeName $count"

        val defaultBlock = when (type) {
            PresetType.IMAGE -> dev.screengoated.toolbox.mobile.shared.preset.imageBlock(
                DEFAULT_IMAGE_MODEL_ID,
                "Extract text from this image.",
            )
            PresetType.TEXT_SELECT, PresetType.TEXT_INPUT -> dev.screengoated.toolbox.mobile.shared.preset.textBlock(
                DEFAULT_TEXT_MODEL_ID,
                "Translate to {language1}. Output ONLY the translation.",
                "language1" to "Vietnamese",
            )
            PresetType.MIC -> dev.screengoated.toolbox.mobile.shared.preset.audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
            )
            PresetType.DEVICE_AUDIO -> dev.screengoated.toolbox.mobile.shared.preset.audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
            )
        }.copy(streamingEnabled = true) // A new Windows preset block uses the streaming default.

        val newPreset = dev.screengoated.toolbox.mobile.shared.preset.Preset(
            id = newId,
            nameEn = name,
            nameVi = name,
            nameKo = name,
            presetType = type,
            blocks = listOf(defaultBlock),
            textInputMode = if (type == PresetType.TEXT_INPUT) "type" else "select",
            audioSource = if (type == PresetType.DEVICE_AUDIO) "device" else "mic",
        )
        val updatedCustom = storedOverrides.customPresets.toMutableMap()
        updatedCustom[newId] = newPreset
        val saved = persistAll(
            builtInOverrides = storedOverrides.builtInOverrides,
            customPresets = updatedCustom,
        )
        return newId.takeIf { saved }
    }

    @Synchronized
    fun duplicatePreset(id: String, lang: String): String? {
        val row = unambiguousVisibleRow(id) ?: return null
        return duplicatePresetRow(row.rowIdentity, lang)
    }

    @Synchronized
    internal fun duplicatePresetRow(rowKey: String, lang: String): String? {
        val source = _catalogState.value.findRow(rowKey)?.preset ?: return null
        val newId = allocatePresetId() ?: return null
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
        val saved = persistAll(
            builtInOverrides = storedOverrides.builtInOverrides,
            customPresets = updatedCustom,
        )
        return newId.takeIf { saved }
    }

    @Synchronized
    fun deletePreset(id: String) {
        val row = unambiguousVisibleRow(id) ?: return
        deletePresetRow(row.rowIdentity)
    }

    @Synchronized
    internal fun deletePresetRow(rowKey: String): Boolean {
        val row = _catalogState.value.findRow(rowKey) ?: return false
        val id = row.preset.id
        return if (row.isBuiltIn) {
            // Built-in: mark as hidden (will re-appear on reset)
            val updatedOverrides = storedOverrides.builtInOverrides.toMutableMap()
            val existing = updatedOverrides[id] ?: PresetOverride()
            updatedOverrides[id] = existing.copy(isHidden = true)
            persistOverrides(updatedOverrides)
        } else {
            // Custom: remove from custom presets
            val updatedCustom = storedOverrides.customPresets.toMutableMap()
            val storageKey = row.rowKey.removePrefix("custom:")
            if (updatedCustom.remove(storageKey) == null) return false
            persistAll(
                builtInOverrides = storedOverrides.builtInOverrides,
                customPresets = updatedCustom,
            )
        }
    }

    @Synchronized
    fun restoreBuiltInPreset(id: String) {
        if (!canonicalById.containsKey(id)) {
            return
        }
        val updated = storedOverrides.builtInOverrides.toMutableMap().also { it.remove(id) }
        persistOverrides(updated)
    }

    @Synchronized
    internal fun restorePresetRow(rowIdentity: String): Boolean {
        val row = _catalogState.value.findRow(rowIdentity) ?: return false
        if (!row.isBuiltIn || !canonicalById.containsKey(row.preset.id)) return false
        val updated = storedOverrides.builtInOverrides.toMutableMap().also { it.remove(row.preset.id) }
        return persistOverrides(updated)
    }

    @Synchronized
    fun updateBuiltInOverride(
        id: String,
        mutation: (Preset) -> Preset,
    ): Boolean {
        val row = unambiguousVisibleRow(id) ?: return false
        return updatePresetRow(row.rowIdentity, mutation)
    }

    @Synchronized
    internal fun updatePresetRow(rowKey: String, mutation: (Preset) -> Preset): Boolean {
        val row = _catalogState.value.findRow(rowKey) ?: return false
        val id = row.preset.id
        if (row.isBuiltIn) {
            val canonical = canonicalById[id] ?: return false
            if (storedOverrides.builtInOverrides[id]?.isHidden == true) return false
            // Built-in preset: store as delta override
            val current = row.preset
            val updatedPreset = mutation(current)
            if (updatedPreset.id != id) return false
            val override = updatedPreset.toOverrideComparedTo(canonical)
            val updatedOverrides = storedOverrides.builtInOverrides.toMutableMap()
            if (override.isEmpty()) {
                updatedOverrides.remove(id)
            } else {
                updatedOverrides[id] = override
            }
            return persistOverrides(updatedOverrides)
        } else {
            // Custom preset: update the full preset in customPresets
            val storageKey = row.rowKey.removePrefix("custom:")
            val existing = storedOverrides.customPresets[storageKey] ?: return false
            val updatedPreset = mutation(existing)
            if (updatedPreset.id != existing.id) return false
            val updatedCustom = storedOverrides.customPresets.toMutableMap()
            updatedCustom[storageKey] = updatedPreset
            return persistAll(
                builtInOverrides = storedOverrides.builtInOverrides,
                customPresets = updatedCustom,
            )
        }
    }

    private fun allocatePresetId(): String? {
        repeat(32) {
            val candidate = try {
                newPresetId()
            } catch (_: Exception) {
                toastBus.show(presetPersistenceMessage(uiLanguage(), PresetPersistenceFailure.WRITE))
                return null
            }
            if (candidate.isNotBlank() && !candidate.startsWith("preset_") &&
                !canonicalById.containsKey(candidate) &&
                !storedOverrides.builtInOverrides.containsKey(candidate) &&
                !storedOverrides.customPresets.containsKey(candidate) &&
                storedOverrides.customPresets.values.none { it.id == candidate }) return candidate
        }
        toastBus.show(presetPersistenceMessage(uiLanguage(), PresetPersistenceFailure.WRITE))
        return null
    }

    fun executePreset(
        preset: Preset,
        input: PresetInput,
    ) = sessionOwnership.transition {
        sessionOwnership.retireEffects()
        executionJob?.cancel()
        executionJob = null
        val capability = resolveExecutionCapability(preset)
        if (!capability.supported) {
            _executionState.value = PresetExecutionState(
                activePresetId = preset.id,
                error = capability.reason?.message() ?: "Preset execution is not ready on Android yet.",
            )
            return@transition
        }

        val sessionId = "preset-session-${nextSessionOrdinal++}"
        _executionState.value = PresetExecutionState(
            sessionId = sessionId,
            isExecuting = true,
            activePresetId = preset.id,
        )
        sessionOwnership.activateEffects(sessionId)

        executionJob = scope.launch {
            try {
                when (input) {
                    is PresetInput.Text,
                    is PresetInput.Image,
                    is PresetInput.Audio,
                    -> Unit
                }

                graphExecutor.executeGraph(
                    sessionId = sessionId,
                    preset = preset,
                    input = input,
                )

                _executionState.update { it.finishSession(sessionId) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _executionState.update { it.finishSession(sessionId, e.message ?: "Execution failed") }
            }
        }
    }

    internal fun refineInPlace(
        originalModelId: String,
        previousText: String,
        refinePrompt: String,
        onChunk: (String) -> Unit,
        onModelSelected: (PresetModelDescriptor) -> Unit,
        onComplete: (Result<PresetRefineResult>) -> Unit,
    ): Job {
        val keys = apiKeys()
        val settings = runtimeSettings()
        val initialModel = initialRefineModel(originalModelId, settings, keys)
            ?: return scope.launch { onComplete(Result.failure(Exception("No text model configured"))) }
        val lang = uiLanguage()

        // FIFO on the main dispatcher, like Handler.post; JVM tests inject a test dispatcher.
        fun postMain(action: () -> Unit) { scope.launch { action() } }
        return scope.launch(Dispatchers.IO) {
            val request = presetRefineTextRequest(previousText, refinePrompt)
            val failedModelIds = mutableListOf<String>()
            val blockedProviders = linkedSetOf<PresetModelProvider>()
            var currentModel = initialModel
            val result: Result<PresetRefineResult> = runCatching {
                var completed: PresetRefineResult? = null
                while (completed == null) {
                    val modelAttempt = PresetModelAttempt(currentModel.id)
                    val preflight = preflightSkipReason(
                        modelId = currentModel.id,
                        provider = currentModel.provider,
                        apiKeys = keys,
                        blockedProviders = blockedProviders,
                        settings = settings,
                    ) ?: claimPresetModelAttempt(modelAttempt)
                    if (preflight != null) {
                        if (shouldBlockRetryProvider(preflight)) {
                            blockedProviders += currentModel.provider
                        }
                        failedModelIds += currentModel.id
                        currentModel = resolveNextRetryModel(
                            currentModelId = currentModel.id,
                            failedModelIds = failedModelIds,
                            blockedProviders = blockedProviders,
                            chainKind = PresetRetryChainKind.TEXT_TO_TEXT,
                            apiKeys = keys,
                            settings = settings,
                            searchEnabled = false,
                        ) ?: error(preflight)
                        continue
                    }

                    val attemptedModel = currentModel
                    postMain { onModelSelected(attemptedModel) }
                    val attempt = try { textApiClient.executeStreaming(
                        modelId = attemptedModel.id,
                        prompt = request.prompt,
                        inputText = request.inputText,
                        apiKeys = keys,
                        uiLanguage = lang,
                        searchLabel = null,
                        onChunk = { chunk -> postMain { onChunk(chunk) } },
                        streamingEnabled = true,
                    ) } catch (error: Exception) {
                        releasePresetModelProbe(modelAttempt)
                        throw error
                    }
                    val error = attempt.exceptionOrNull()
                    if (error == null) {
                        recordPresetModelSuccess(modelAttempt)
                        completed = PresetRefineResult(
                            text = attempt.getOrThrow(),
                            modelId = attemptedModel.id,
                            modelProvider = attemptedModel.provider,
                        )
                        continue
                    }
                    if (error is CancellationException) {
                        releasePresetModelProbe(modelAttempt)
                        throw error
                    }
                    val message = error.message ?: "Refine failed"
                    recordPresetModelFailure(modelAttempt, message)
                    if (!shouldAdvanceRetryChain(message)) throw error
                    if (shouldBlockRetryProvider(message)) {
                        blockedProviders += attemptedModel.provider
                    }
                    failedModelIds += attemptedModel.id
                    currentModel = resolveNextRetryModel(
                        currentModelId = attemptedModel.id,
                        failedModelIds = failedModelIds,
                        blockedProviders = blockedProviders,
                        chainKind = PresetRetryChainKind.TEXT_TO_TEXT,
                        apiKeys = keys,
                        settings = settings,
                        searchEnabled = false,
                    ) ?: throw error
                    postMain { onChunk(TextApiClient.WIPE_SIGNAL) }
                }
                requireNotNull(completed)
            }
            kotlinx.coroutines.withContext(mainDispatcher) {
                onComplete(result)
            }
        }
    }

    fun cancelExecution() = sessionOwnership.transition {
        sessionOwnership.retireEffects()
        _executionState.update {
            it.copy(isExecuting = false)
        }
        executionJob?.cancel()
        executionJob = null
    }

    fun resetState() = sessionOwnership.transition {
        sessionOwnership.retireEffects()
        _executionState.value = PresetExecutionState()
        executionJob?.cancel()
        executionJob = null
    }

    @Synchronized
    private fun publishCatalog() {
        val activeKeys = canonicalPresets.asSequence()
            .filter { storedOverrides.builtInOverrides[it.id]?.isHidden != true }
            .map { builtInPresetRowKey(it.id) }
            .plus(storedOverrides.customPresets.keys.asSequence().map(::customPresetRowKey))
            .toSet()
        rowIncarnations.keys.retainAll(activeKeys)
        fun incarnation(key: String): Long = rowIncarnations.getOrPut(key) {
            nextRowIncarnation = Math.addExact(nextRowIncarnation, 1)
            nextRowIncarnation
        }
        val builtInResolved = canonicalPresets.mapNotNull { canonical ->
            val override = storedOverrides.builtInOverrides[canonical.id]
            if (override?.isHidden == true) return@mapNotNull null
            val resolved = override?.let { canonical.applyOverride(it) } ?: canonical
            val executionCapability = resolveExecutionCapability(resolved)
            ResolvedPreset(
                preset = resolved,
                hasOverride = override != null,
                isBuiltIn = true,
                rowEpoch = rowEpoch,
                rowIncarnation = incarnation(builtInPresetRowKey(canonical.id)),
                executionCapability = executionCapability,
                placeholderReasons = resolvePlaceholderReasons(
                    preset = resolved,
                    executionCapability = executionCapability,
                ),
            )
        }
        val customResolved = storedOverrides.customPresets.map { (storageKey, preset) ->
            val resolvedPreset = preset
            val executionCapability = resolveExecutionCapability(resolvedPreset)
            ResolvedPreset(
                preset = resolvedPreset,
                hasOverride = false,
                isBuiltIn = false,
                rowKey = customPresetRowKey(storageKey),
                rowEpoch = rowEpoch,
                rowIncarnation = incarnation(customPresetRowKey(storageKey)),
                executionCapability = executionCapability,
                placeholderReasons = resolvePlaceholderReasons(
                    preset = resolvedPreset,
                    executionCapability = executionCapability,
                ),
            )
        }
        _catalogState.value = PresetCatalogState(
            presets = builtInResolved + customResolved,
            persistenceFailure = persistenceFailure,
        )
    }

    private fun persistOverrides(updatedOverrides: Map<String, PresetOverride>): Boolean =
        persistAll(builtInOverrides = updatedOverrides, customPresets = storedOverrides.customPresets)

    private fun persistAll(
        builtInOverrides: Map<String, PresetOverride>,
        customPresets: Map<String, Preset>,
    ): Boolean {
        val updated = StoredPresetOverrides(
            version = storedOverrides.version,
            builtInOverrides = builtInOverrides,
            customPresets = customPresets,
        )
        if (persistenceFailure == PresetPersistenceFailure.READ) {
            toastBus.show(presetPersistenceMessage(uiLanguage(), PresetPersistenceFailure.READ))
            return false
        }
        try {
            overrideStore.save(updated)
        } catch (_: Exception) {
            persistenceFailure = PresetPersistenceFailure.WRITE
            publishCatalog()
            toastBus.show(presetPersistenceMessage(uiLanguage(), PresetPersistenceFailure.WRITE))
            return false
        }
        storedOverrides = updated
        persistenceFailure = null
        publishCatalog()
        return true
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
