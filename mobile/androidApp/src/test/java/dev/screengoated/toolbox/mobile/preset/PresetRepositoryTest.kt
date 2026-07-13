package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

@OptIn(ExperimentalCoroutinesApi::class)
class PresetRepositoryTest {
    private val json = Json { ignoreUnknownKeys = true }
    private val dispatcher = StandardTestDispatcher()

    @Test
    fun fixtureOverrideMergeAndRestoreMatchRepository() {
        val cases = fixtureCases()
        val mergeCase = cases.first { it.name == "favorite_override_merges_onto_builtin" }
        val restoreCase = cases.first { it.name == "restore_default_removes_override" }

        val mergeStore = InMemoryPresetOverrideStore(
            StoredPresetOverrides(
                builtInOverrides = mapOf(
                    mergeCase.presetId to mergeCase.override!!.toPresetOverride(),
                ),
            ),
        )
        val mergeRepository = createRepository(mergeStore)
        val mergedPreset = requireNotNull(mergeRepository.getResolvedPreset(mergeCase.presetId)).preset
        assertTrue(mergedPreset.isFavorite)

        val restoreStore = InMemoryPresetOverrideStore(
            StoredPresetOverrides(
                builtInOverrides = mapOf(
                    restoreCase.presetId to restoreCase.override!!.toPresetOverride(),
                ),
            ),
        )
        val restoreRepository = createRepository(restoreStore)
        restoreRepository.restoreBuiltInPreset(restoreCase.presetId)
        val restoredPreset = requireNotNull(
            restoreRepository.getResolvedPreset(restoreCase.presetId),
        ).preset

        assertFalse(restoredPreset.isFavorite)
        assertEquals("fixed", restoredPreset.promptMode)
        assertFalse(
            restoreStore.load().builtInOverrides.containsKey(restoreCase.presetId),
        )
    }

    @Test
    fun fixtureExecutionCapabilitiesMatchRepository() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        fixtureCases()
            .filter { it.expectedExecution != null }
            .forEach { case ->
                val resolved = requireNotNull(repository.getResolvedPreset(case.presetId))
                assertEquals(
                    case.expectedExecution!!.supported,
                    resolved.executionCapability.supported,
                )
                assertEquals(
                    case.expectedExecution.reason,
                    resolved.executionCapability.reason?.name,
                )
            }
    }

    @Test
    fun favoriteTogglePersistsAsBuiltInOverride() {
        val store = InMemoryPresetOverrideStore()
        val repository = createRepository(store)

        repository.toggleFavorite("preset_translate")

        val resolved = requireNotNull(repository.getResolvedPreset("preset_translate"))
        assertTrue(resolved.preset.isFavorite)
        assertTrue(resolved.hasOverride)
        assertEquals(
            true,
            store.load().builtInOverrides["preset_translate"]?.isFavorite,
        )
    }

    @Test
    fun windowGeometryOverridePersistsAsBuiltInOverride() {
        val store = InMemoryPresetOverrideStore()
        val repository = createRepository(store)

        repository.updateBuiltInOverride("preset_ask_ai") { preset ->
            preset.copy(
                windowGeometry = dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry(
                    x = 120,
                    y = 180,
                    width = 420,
                    height = 320,
                ),
            )
        }

        val resolved = requireNotNull(repository.getResolvedPreset("preset_ask_ai")).preset
        assertEquals(120, resolved.windowGeometry?.x)
        assertEquals(180, resolved.windowGeometry?.y)
        assertEquals(420, store.load().builtInOverrides["preset_ask_ai"]?.windowGeometry?.width)
    }

    @Test
    fun htmlOutputTextPresetIsExecutable() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_make_game"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.HTML_RESULT_NOT_READY))
    }

    @Test
    fun quickNotePresetIsSupported() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_quick_note"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY))
    }

    @Test
    fun hangImagePresetIsNoLongerUpcoming() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_hang_image"))

        assertFalse(resolved.preset.isUpcoming)
        assertTrue(resolved.executionCapability.supported)
    }

    @Test
    fun micAudioPresetIsSupported() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_transcribe"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY))
        assertEquals(
            "Transcribe the audio into text. Output ONLY the transcript.",
            resolved.preset.blocks.first().prompt,
        )
    }

    @Test
    fun realtimeAudioPresetIsSupported() {
        val repository = createRepository(InMemoryPresetOverrideStore())

        val resolved = requireNotNull(repository.getResolvedPreset("preset_realtime_audio_translate"))

        assertTrue(resolved.executionCapability.supported)
        assertFalse(resolved.placeholderReasons.contains(PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY))
    }

    @Test
    fun deviceAudioBuiltInsKeepWindowsSourceAndRealtimeFlags() {
        val realtimePreset = requireNotNull(
            DefaultPresetLookup.byId("preset_realtime_audio_translate"),
        )
        val deviceRecordPreset = requireNotNull(
            DefaultPresetLookup.byId("preset_record_device"),
        )

        assertEquals("device", realtimePreset.audioSource)
        assertEquals("realtime", realtimePreset.audioProcessingMode)
        assertEquals("device", deviceRecordPreset.audioSource)
    }

    @Test
    fun audioRuntimeFixtureMatchesAndroidAudioDefaultsAndExplicitGaps() {
        val fixture = audioRuntimeFixture()
        val repository = createRepository(InMemoryPresetOverrideStore())
        val thresholds = fixture.getValue("auto_stop_thresholds").jsonObject

        assertEquals(0.001f, thresholds.getValue("warmup_rms").jsonPrimitive.float)
        assertEquals(0.015f, thresholds.getValue("speech_rms").jsonPrimitive.float)
        assertEquals(800, thresholds.getValue("silence_ms").jsonPrimitive.int)
        assertEquals(2000, thresholds.getValue("min_speech_ms").jsonPrimitive.int)

        val autoStopIds = fixture.getValue("default_auto_stop_presets").jsonArray
            .map { it.jsonPrimitive.content }
        autoStopIds.forEach { presetId ->
            assertEquals(
                "autoStopRecording for $presetId",
                true,
                requireNotNull(DefaultPresetLookup.byId(presetId)).autoStopRecording,
            )
        }

        val micContract = fixture.getValue("shared_mic_button_contract").jsonObject
        val transcribePreset = requireNotNull(
            DefaultPresetLookup.byId(micContract.getValue("text_input").jsonPrimitive.content),
        )
        assertEquals(
            micContract.getValue("preset_transcribe_prompt").jsonPrimitive.content,
            transcribePreset.blocks.first().prompt,
        )

        fixture.getValue("android_explicit_unsupported_presets").jsonArray.forEach { element ->
            val gap = element.jsonObject
            val resolved = requireNotNull(
                repository.getResolvedPreset(gap.getValue("preset_id").jsonPrimitive.content),
            )
            assertEquals(false, resolved.executionCapability.supported)
            assertEquals(
                gap.getValue("reason").jsonPrimitive.content,
                resolved.executionCapability.reason?.name,
            )
            assertEquals(
                gap.getValue("model_id").jsonPrimitive.content,
                resolved.preset.blocks.first { block ->
                    block.blockType == dev.screengoated.toolbox.mobile.shared.preset.BlockType.AUDIO
                }.model,
            )
        }
    }

    @Test
    fun audioRuntimeFixtureContractsAreBackedByAndroidSources() {
        val fixture = audioRuntimeFixture()
        assertEquals(
            setOf(
                "canonical_windows_files",
                "recording_toggle_contract",
                "auto_stop_thresholds",
                "default_auto_stop_presets",
                "recording_shell_contract",
                "shared_mic_button_contract",
                "streaming_capture_contract",
                "android_explicit_unsupported_presets",
                "realtime_contract",
                "android_launch_contract",
                "bubble_capture_host_contract",
                "auto_speak_contract",
            ),
            fixture.keys,
        )

        val audioSession = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetAudioCaptureSession.kt")
        // PresetOverlayController was split into focused sibling files; read them
        // all so the behavior-contract assertions resolve wherever the code lives.
        val overlayController = listOf(
            "PresetOverlayController.kt",
            "PresetOverlayControllerAudio.kt",
            "PresetOverlayControllerImage.kt",
            "PresetOverlayControllerClipboard.kt",
            "PresetOverlayControllerLaunch.kt",
        ).joinToString("\n") {
            repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/$it")
        }
        val buildGradle = repoFile("mobile/androidApp/build.gradle.kts")
        val recordingUi = repoFile("src/overlay/recording/ui.rs")
        val audioBlockExecutor = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/PresetAudioBlockExecutor.kt")
        val graphExecutor = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/PresetGraphExecutor.kt")
        val autoSpeak = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetAutoSpeakCoordinator.kt")
        val foregroundSupport = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/BubbleForegroundSupport.kt")

        val toggleContract = fixture.getValue("recording_toggle_contract").jsonObject
        assertEquals("start_recording", toggleContract.getValue("first_launch").jsonPrimitive.content)
        assertEquals("stop_and_submit", toggleContract.getValue("second_launch_same_preset").jsonPrimitive.content)
        assertEquals("abort_and_close", toggleContract.getValue("third_launch_while_processing").jsonPrimitive.content)
        assertTrue(audioSession.contains("fun toggleOrAbortIfMatching(presetId: String): Boolean"))
        assertTrue(audioSession.contains("""if (state == "processing")"""))
        assertTrue(audioSession.contains("val onCancelled = onCancelledCallback"))
        assertTrue(audioSession.contains("onCancelled?.invoke()"))
        assertTrue(audioSession.contains("cancel()"))
        assertTrue(audioSession.contains("stopAndSubmit()"))
        assertTrue(overlayController.contains("if (audioCaptureSession.toggleOrAbortIfMatching(presetId))"))

        val shellContract = fixture.getValue("recording_shell_contract").jsonObject
        assertEquals("windows_webview_template", shellContract.getValue("source").jsonPrimitive.content)
        assertTrue(buildGradle.contains("""repoRoot.resolve("src/overlay/recording/ui.rs")"""))
        assertTrue(buildGradle.contains("windows_recording_template.html"))
        assertTrue(buildGradle.contains("{{BRIDGE_PRELUDE}}"))
        assertTrue(buildGradle.contains("{{MOBILE_SHIM}}"))
        shellContract.getValue("bridge_methods").jsonArray
            .map { it.jsonPrimitive.content }
            .forEach { method ->
                assertTrue("Windows recording bridge method missing: $method", recordingUi.contains(method))
            }
        shellContract.getValue("ipc_messages").jsonArray
            .map { it.jsonPrimitive.content }
            .forEach { message ->
                assertTrue("Windows recording IPC missing: $message", recordingUi.contains(message))
            }

        val streamingContract = fixture.getValue("streaming_capture_contract").jsonObject
        assertEquals(
            "partial_transcript_during_capture",
            streamingContract.getValue("gemini-live-audio").jsonPrimitive.content,
        )
        assertEquals(
            "first_audio_block_uses_precomputed_transcript",
            streamingContract.getValue("final_transcript_handoff").jsonPrimitive.content,
        )
        assertEquals(
            "inject_deltas_during_capture_and_skip_final_paste_when_already_written",
            streamingContract.getValue("streaming_auto_paste").jsonPrimitive.content,
        )
        assertTrue(audioSession.contains("PresetModelProvider.GEMINI_LIVE"))
        assertTrue(audioSession.contains("openStreamingSession"))
        assertTrue(audioSession.contains("onStreamingTextChunk(chunk)"))
        assertTrue(audioSession.contains("precomputedTranscript = streamingTranscript?.transcript"))
        assertTrue(audioSession.contains("isStreamingResult = streamingTranscript?.producedRealtimePaste == true"))
        assertTrue(audioBlockExecutor.contains("input.precomputedTranscript"))
        assertTrue(graphExecutor.contains("shouldSkipFinalAutoPaste"))
        assertTrue(graphExecutor.contains("(input as? PresetInput.Audio)?.isStreamingResult == true"))

        val realtimeContract = fixture.getValue("realtime_contract").jsonObject
        assertEquals("transient_live_service_override", realtimeContract.getValue("launch_path").jsonPrimitive.content)
        assertTrue(realtimeContract.getValue("restore_saved_config_on_stop").jsonPrimitive.boolean)
        assertTrue(overlayController.contains("launchRealtimeAudioPreset"))
        assertTrue(overlayController.contains("isTransientSessionConfigActive()"))
        assertTrue(overlayController.contains("setActiveRealtimePresetId"))
        assertTrue(overlayController.contains("LiveTranslateService.stop(context)"))
        assertTrue(overlayController.contains("MainActivity.EXTRA_RESUME_PENDING_AUDIO_PRESET"))

        val launchContract = fixture.getValue("android_launch_contract").jsonObject
        assertEquals(
            "gate_before_capture_and_open_accessibility_settings",
            launchContract.getValue("audio_autopaste_without_accessibility").jsonPrimitive.content,
        )
        assertEquals(
            "gate_before_surface_suppression_and_open_accessibility_settings",
            launchContract.getValue("image_capture_without_accessibility").jsonPrimitive.content,
        )
        assertEquals(
            "copy_from_app_context_not_accessibility_service",
            launchContract.getValue("image_autocopy").jsonPrimitive.content,
        )
        assertTrue(overlayController.contains("requiresAccessibilityForAudioAutoPaste"))
        assertTrue(overlayController.contains("openAccessibilitySettings()"))
        assertTrue(overlayController.contains("currentScreenshotSupport().failureReason"))
        assertTrue(overlayController.contains("copyImageToClipboard(pngBytes)"))

        val foregroundContract = fixture.getValue("bubble_capture_host_contract").jsonObject
        assertEquals("specialUse", foregroundContract.getValue("default_foreground_mode").jsonPrimitive.content)
        assertEquals("promote_to_microphone_then_restore", foregroundContract.getValue("mic_presets").jsonPrimitive.content)
        assertEquals("promote_to_mediaProjection_then_restore", foregroundContract.getValue("device_audio_presets").jsonPrimitive.content)
        assertTrue(foregroundSupport.contains("PresetAudioForegroundMode.NONE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE"))
        assertTrue(foregroundSupport.contains("PresetAudioForegroundMode.MICROPHONE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE"))
        assertTrue(foregroundSupport.contains("PresetAudioForegroundMode.MEDIA_PROJECTION -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION"))
        assertTrue(overlayController.contains("PresetAudioForegroundMode.MICROPHONE"))
        assertTrue(overlayController.contains("PresetAudioForegroundMode.MEDIA_PROJECTION"))
        assertTrue(overlayController.contains("onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)"))

        val autoSpeakContract = fixture.getValue("auto_speak_contract").jsonObject
        assertEquals("AUTO_SPEAK", autoSpeakContract.getValue("consumer").jsonPrimitive.content)
        assertEquals("single_retry_then_surface_error", autoSpeakContract.getValue("first_failure_recovery").jsonPrimitive.content)
        assertTrue(autoSpeak.contains("consumer = TtsConsumer.AUTO_SPEAK"))
        assertTrue(autoSpeak.contains("MAX_AUTO_SPEAK_RETRIES = 1"))
        assertTrue(autoSpeak.contains("pending.retryCount + 1"))
        assertTrue(autoSpeak.contains("Could not speak the preset result."))
    }

    @Test
    fun presetModelCatalogIncludesGemma4FamilyAcrossModalities() {
        val textModel = requireNotNull(PresetModelCatalog.getById("gemma-4-26b-a4b"))
        val visionModel = requireNotNull(PresetModelCatalog.getById("gemma-4-26b-a4b-vision"))

        assertEquals(PresetModelProvider.GOOGLE, textModel.provider)
        assertEquals(PresetModelType.TEXT, textModel.modelType)
        assertEquals(PresetModelType.VISION, visionModel.modelType)
        assertTrue(PresetModelCatalog.forType(PresetModelType.TEXT).any { it.id == "gemma-4-31b" })
        assertTrue(PresetModelCatalog.forType(PresetModelType.VISION).any { it.id == "gemma-4-31b-vision" })
        val cerebrasVision = requireNotNull(
            PresetModelCatalog.getById("gemma-4-31b-cerebras-vision"),
        )
        assertEquals(PresetModelProvider.CEREBRAS, cerebrasVision.provider)
        assertEquals("gemma-4-31b", cerebrasVision.fullName)
        assertTrue(PresetModelCatalog.getById("gemma-4-26b-a4b-audio") == null)
        assertTrue(PresetModelCatalog.getById("gemma-4-31b-audio") == null)
    }

    @Test
    fun imagePresetWithAudioBlockIsRejectedAsUnsupported() {
        val capability = PresetExecutionCapabilityResolver().resolveExecutionCapability(
            dev.screengoated.toolbox.mobile.shared.preset.Preset(
                id = "image-audio-mixed",
                nameEn = "Image Audio Mixed",
                nameVi = "Image Audio Mixed",
                nameKo = "Image Audio Mixed",
                presetType = dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE,
                blocks = listOf(
                    dev.screengoated.toolbox.mobile.shared.preset.audioBlock("whisper-fast"),
                ),
            ),
        )

        assertFalse(capability.supported)
        assertEquals(PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY, capability.reason)
    }

    private fun createRepository(store: PresetOverrideStore): PresetRepository {
        return PresetRepository(
            textApiClient = TextApiClient(OkHttpClient()),
            visionApiClient = VisionApiClient(OkHttpClient()),
            apiKeys = { ApiKeys() },
            runtimeSettings = { PresetRuntimeSettings() },
            uiLanguage = { "en" },
            overrideStore = store,
            toastBus = AppToastBus(),
            mainDispatcher = dispatcher,
        )
    }

    private fun fixtureCases(): List<FixtureCase> {
        val root = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        return root.getValue("cases").jsonArray.map { case ->
            val jsonObject = case.jsonObject
            FixtureCase(
                name = jsonObject.getValue("name").jsonPrimitive.content,
                presetId = jsonObject.getValue("preset_id").jsonPrimitive.content,
                override = jsonObject["override"]?.jsonObject,
                expectedExecution = jsonObject["expected_execution"]?.jsonObject?.let { execution ->
                    FixtureExecution(
                        supported = execution.getValue("supported").jsonPrimitive.boolean,
                        reason = execution["reason"]?.jsonPrimitive?.contentOrNull,
                    )
                },
            )
        }
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "catalog-overrides.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "catalog-overrides.json"),
            Paths.get("parity-fixtures", "preset-system", "catalog-overrides.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate preset parity fixture.")
    }

    private fun audioRuntimeFixture(): JsonObject {
        return json.parseToJsonElement(Files.readAllBytes(audioRuntimeFixturePath()).decodeToString()).jsonObject
    }

    private fun audioRuntimeFixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "audio-runtime.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "audio-runtime.json"),
            Paths.get("parity-fixtures", "preset-system", "audio-runtime.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Could not locate audio-runtime parity fixture.")
    }

    private fun repoFile(path: String): String = File(repoRoot(), path).readText()

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile
        }.first { root ->
            File(root, "parity-fixtures/preset-system/audio-runtime.json").exists()
        }
    }

    private data class FixtureCase(
        val name: String,
        val presetId: String,
        val override: JsonObject?,
        val expectedExecution: FixtureExecution?,
    )

    private data class FixtureExecution(
        val supported: Boolean,
        val reason: String?,
    )

    private class InMemoryPresetOverrideStore(
        private var state: StoredPresetOverrides = StoredPresetOverrides(),
    ) : PresetOverrideStore {
        override fun load(): StoredPresetOverrides = state

        override fun save(overrides: StoredPresetOverrides) {
            state = overrides
        }
    }
}

private object DefaultPresetLookup {
    private val byId = dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets.all.associateBy { it.id }

    fun byId(id: String): dev.screengoated.toolbox.mobile.shared.preset.Preset? = byId[id]
}

private fun JsonObject.toPresetOverride(): PresetOverride {
    return PresetOverride(
        isFavorite = this["is_favorite"]?.jsonPrimitive?.booleanOrNull,
        promptMode = this["prompt_mode"]?.jsonPrimitive?.contentOrNull,
    )
}
