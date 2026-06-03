package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Rect
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import android.widget.Toast
import androidx.core.content.FileProvider
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchKind
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchRequest
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.resolvePrompt
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.service.ScreenshotCaptureFailureReason
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import dev.screengoated.toolbox.mobile.service.LiveTranslateService
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import java.io.File
import kotlin.math.roundToInt

internal class PresetOverlayController(
    internal val context: Context,
    internal val scope: CoroutineScope,
    internal val windowManager: WindowManager,
    internal val presetRepository: PresetRepository,
    internal val uiPreferencesFlow: StateFlow<MobileUiPreferences>,
    internal val uiPreferencesProvider: () -> MobileUiPreferences,
    internal val keepOpenProvider: () -> Boolean,
    internal val onKeepOpenChanged: (Boolean) -> Unit,
    internal val onIncreaseBubbleSize: () -> Unit,
    internal val onDecreaseBubbleSize: () -> Unit,
    internal val onPanelExpandedChanged: (Boolean) -> Unit = {},
    internal val onBubbleSuppressedChanged: (Boolean) -> Unit = {},
    internal val onRequestBubbleFront: () -> Unit = {},
    internal val onAudioCaptureForegroundModeChanged: (PresetAudioForegroundMode) -> Unit = {},
    internal val ttsRuntimeService: TtsRuntimeService? = null,
    internal val ttsSettingsSnapshotProvider: (() -> dev.screengoated.toolbox.mobile.service.tts.TtsRequestSettingsSnapshot)? = null,
) {
    internal val appContainer = (context.applicationContext as SgtMobileApplication).appContainer
    internal val favoriteBubbleHtmlBuilder = FavoriteBubbleHtmlBuilder()
    internal val textInputHtmlBuilder = PresetTextInputHtmlBuilder()
    internal val density = context.resources.displayMetrics.density
    internal val dismissTarget = PresetOverlayDismissTarget(context, windowManager, ::uiLanguage)
    internal val clipboardManager = context.getSystemService(ClipboardManager::class.java)

    internal val processingIndicator = PresetProcessingIndicator(context, windowManager)
    internal val accessibilityDisclosure = AccessibilityDisclosureOverlay(context, windowManager)
    internal val imageCaptureSession = PresetImageCaptureSession(
        context = context,
        windowManager = windowManager,
        uiLanguage = ::uiLanguage,
        onBubbleSuppressedChanged = onBubbleSuppressedChanged,
        onOverlaySuppressedChanged = ::setOverlayChromeSuppressed,
    )
    internal val audioCaptureSession = PresetAudioCaptureSession(
        context = context,
        windowManager = windowManager,
        projectionConsentStore = appContainer.projectionConsentStore,
        audioApiClient = appContainer.audioApiClient,
        uiLanguage = ::uiLanguage,
        isDarkTheme = ::isDarkTheme,
        permissionSnapshotProvider = { appContainer.repository.state.value.permissions },
        screenBoundsProvider = ::screenBounds,
        toastBus = appContainer.toastBus,
        onStreamingTextChunk = ::appendStreamingTextChunk,
    )
    internal val autoSpeakCoordinator = if (ttsRuntimeService != null && ttsSettingsSnapshotProvider != null) {
        PresetAutoSpeakCoordinator(
            context = context,
            ttsRuntimeService = ttsRuntimeService,
            snapshotProvider = ttsSettingsSnapshotProvider,
            uiLanguage = ::uiLanguage,
        )
    } else {
        null
    }
    internal var activePreset: ResolvedPreset? = null
    internal var bubbleBounds = OverlayBounds(x = 0, y = 0, width = dp(48), height = dp(48))
    internal var imageContinuousPresetId: String? = null
    internal var imageContinuousRearmPending = false
    internal var nextImageCaptureTraceId = 1L

    internal val panelModule = PresetOverlayPanelModule(
        context = context,
        windowManager = windowManager,
        favoriteBubbleHtmlBuilder = favoriteBubbleHtmlBuilder,
        uiLanguage = ::uiLanguage,
        isDarkTheme = { isDarkTheme() },
        keepOpenProvider = keepOpenProvider,
        onKeepOpenChanged = onKeepOpenChanged,
        onIncreaseBubbleSize = onIncreaseBubbleSize,
        onDecreaseBubbleSize = onDecreaseBubbleSize,
        onPanelExpandedChanged = onPanelExpandedChanged,
        onRequestBubbleFront = onRequestBubbleFront,
        bubbleBoundsProvider = { bubbleBounds },
        screenBoundsProvider = ::screenBounds,
        density = density,
        cssToPhysical = ::cssToPhysical,
        dp = ::dp,
        favoritePanelPresets = ::favoritePanelPresets,
        resolvedPresetById = { presetId -> presetRepository.getResolvedPreset(presetId) },
        launchPreset = ::launchPreset,
    )
    internal lateinit var inputModule: PresetOverlayInputModule
    internal lateinit var resultModule: PresetOverlayResultModule

    internal var catalogJob: Job? = null
    internal var executionJob: Job? = null
    internal var uiPreferencesJob: Job? = null
    internal var ttsEventsJob: Job? = null
    internal var lastUiPreferences: MobileUiPreferences = uiPreferencesProvider()

    init {
        resultModule = PresetOverlayResultModule(
            context = context,
            windowManager = windowManager,
            presetRepository = presetRepository,
            dismissTarget = dismissTarget,
            resultHtmlBuilder = PresetResultHtmlBuilder(context),
            buttonCanvasHtmlBuilder = PresetButtonCanvasHtmlBuilder(context),
            renderer = PresetMarkdownRenderer(context),
            uiLanguage = ::uiLanguage,
            isDarkTheme = { isDarkTheme() },
            screenBoundsProvider = ::screenBounds,
            dp = ::dp,
            cssToPhysical = ::cssToPhysical,
            onRequestInputFront = { inputModule.bringToFront() },
            onDismissAll = ::dismissAllOverlays,
            onNoOverlaysRemaining = {
                if (!::inputModule.isInitialized || !inputModule.hasWindow()) {
                    activePreset = null
                }
            },
            onMicRequested = ::launchDefaultMicPreset,
            ttsRuntimeService = ttsRuntimeService,
            ttsSettingsSnapshotProvider = ttsSettingsSnapshotProvider,
            overlayOpacityProvider = { uiPreferencesProvider().overlayOpacityPercent.coerceIn(10, 100) },
        )
        inputModule = PresetOverlayInputModule(
            context = context,
            windowManager = windowManager,
            textInputHtmlBuilder = textInputHtmlBuilder,
            dismissTarget = dismissTarget,
            uiLanguage = ::uiLanguage,
            isDarkTheme = { isDarkTheme() },
            screenBoundsProvider = ::screenBounds,
            dp = ::dp,
            onSubmit = ::submitInput,
            onDismissAll = ::dismissAllOverlays,
            onInputClosedWithoutResults = ::handleInputClosedWithoutResults,
            hasResults = { resultModule.hasResults() },
            onMicRequested = ::launchDefaultMicPreset,
        )
        // Wire centralized post-processing actions (matches Windows step.rs)
        presetRepository.postProcessActions = object : dev.screengoated.toolbox.mobile.preset.PresetPostProcessActions {
            override fun handleAutoCopy(block: dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock, resultText: String) {
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    val copied = copyTextToClipboard(resultText)
                    Toast.makeText(
                        context,
                        if (copied) {
                            localized(
                                "Transcript copied.",
                                "Đã sao chép bản chép lời.",
                                "받아쓴 내용을 복사했습니다.",
                            )
                        } else {
                            localized(
                                "Could not copy transcript.",
                                "Không thể sao chép bản chép lời.",
                                "받아쓴 내용을 복사할 수 없습니다.",
                            )
                        },
                        Toast.LENGTH_SHORT,
                    ).show()
                }
            }

            override fun handleAutoCopyImage(
                block: dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock,
                pngBytes: ByteArray,
            ) {
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    val copied = copyImageToClipboard(pngBytes)
                    Toast.makeText(
                        context,
                        if (copied) {
                            localized(
                                "Image copied to clipboard.",
                                "Đã sao chép ảnh vào clipboard.",
                                "이미지를 클립보드에 복사했습니다.",
                            )
                        } else {
                            localized(
                                "Could not copy image to clipboard.",
                                "Không thể sao chép ảnh vào clipboard.",
                                "이미지를 클립보드에 복사할 수 없습니다.",
                            )
                        },
                        Toast.LENGTH_SHORT,
                    ).show()
                }
            }

            override fun handleAutoSpeak(block: dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock, resultText: String, blockIdx: Int) {
                autoSpeakCoordinator?.schedule(resultText, blockIdx)
            }

            override fun handleAutoPaste() {
                android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                    val svc = SgtAccessibilityService.instance
                    val pasted = svc?.pasteIntoFocusedField() == true
                    if (!pasted) {
                        Toast.makeText(
                            context,
                            if (svc == null) {
                                localized(
                                    "Copied, but auto-paste needs Accessibility enabled.",
                                    "Đã sao chép, nhưng tự dán cần bật Trợ năng.",
                                    "복사했지만 자동 붙여넣기에는 접근성 권한이 필요합니다.",
                                )
                            } else {
                                localized(
                                    "Copied, but could not auto-paste. Place the cursor in a text field.",
                                    "Đã sao chép, nhưng không thể tự dán. Hãy đặt con trỏ vào ô nhập văn bản.",
                                    "복사했지만 자동 붙여넣기를 할 수 없습니다. 텍스트 입력칸에 커서를 두세요.",
                                )
                            },
                            Toast.LENGTH_LONG,
                        ).show()
                    }
                }, 200)
            }
        }

        catalogJob = scope.launch(Dispatchers.Main.immediate) {
            presetRepository.catalogState.collectLatest {
                panelModule.refresh()
                activePreset?.preset?.id?.let { activeId ->
                    activePreset = presetRepository.getResolvedPreset(activeId) ?: activePreset
                }
            }
        }
        executionJob = scope.launch(Dispatchers.Main.immediate) {
            presetRepository.executionState.collectLatest(::renderExecutionState)
        }
        uiPreferencesJob = scope.launch(Dispatchers.Main.immediate) {
            uiPreferencesFlow.collectLatest { preferences ->
                val previous = lastUiPreferences
                lastUiPreferences = preferences
                if (previous != preferences) {
                    refreshOverlayPreferences(previous, preferences)
                }
            }
        }
        if (ttsRuntimeService != null) {
            ttsEventsJob = scope.launch(Dispatchers.Main.immediate) {
                launch {
                    ttsRuntimeService.playbackEvents.collect { event ->
                        autoSpeakCoordinator?.handlePlaybackEvent(event)
                        resultModule.handleTtsPlaybackEvent(event.requestId, event.ownerToken, event.completionStatus)
                    }
                }
                launch {
                    ttsRuntimeService.runtimeState.collectLatest { state ->
                        resultModule.handleTtsRuntimeStateChanged(state.isPlaying, state.activeRequestId)
                    }
                }
            }
        }
    }

    fun updateBubbleBounds(bounds: OverlayBounds) {
        bubbleBounds = bounds
        panelModule.updateBubbleBounds()
    }

    fun togglePanel() {
        panelModule.toggle()
    }

    fun dismissPanel() {
        panelModule.dismiss()
    }

    fun destroy() {
        catalogJob?.cancel()
        executionJob?.cancel()
        uiPreferencesJob?.cancel()
        ttsEventsJob?.cancel()
        panelModule.destroy()
        imageCaptureSession.destroy()
        audioCaptureSession.destroy()
        onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
        inputModule.destroy()
        resultModule.destroy()
        dismissTarget.hide()
        accessibilityDisclosure.dismiss()
        autoSpeakCoordinator?.clear()
        activePreset = null
        imageContinuousPresetId = null
        imageContinuousRearmPending = false
        pendingImageBytes = null
        pendingTextSelectInput = null
        presetRepository.resetState()
    }

    internal fun refreshOverlayPreferences(
        previous: MobileUiPreferences,
        current: MobileUiPreferences,
    ) {
        val themeChanged = previous.themeMode != current.themeMode
        val languageChanged = previous.uiLanguage != current.uiLanguage
        if (!themeChanged && !languageChanged) {
            return
        }

        activePreset?.preset?.id?.let { activeId ->
            activePreset = presetRepository.getResolvedPreset(activeId) ?: activePreset
        }

        panelModule.refresh()
        inputModule.refreshForPreferences()
        if (themeChanged || languageChanged) {
            audioCaptureSession.refreshOverlayForPreferences()
        }

        if (themeChanged) {
            resultModule.refreshResultWindowsForTheme()
        }
        if (themeChanged || languageChanged) {
            resultModule.refreshCanvasWindowForPreferences()
        }
    }

    internal fun launchPreset(
        presetId: String,
        closePanel: Boolean,
        continuousMode: Boolean,
    ) {
        if (closePanel) {
            panelModule.dismiss()
        }
        val resolved = presetRepository.getResolvedPreset(presetId) ?: return
        if (audioCaptureSession.toggleOrAbortIfMatching(presetId)) {
            return
        }
        if (!resolved.executionCapability.supported) {
            Toast.makeText(
                context,
                placeholderReasonLabel(
                    resolved.executionCapability.reason ?: PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
                    uiLanguage(),
                ),
                Toast.LENGTH_SHORT,
            ).show()
            return
        }
        if (requiresAccessibilityForAudioAutoPaste(resolved) && !SgtAccessibilityService.isAvailable) {
            promptAccessibilityDisclosure()
            return
        }

        if (imageContinuousPresetId != null && (imageContinuousPresetId != presetId || !continuousMode)) {
            stopImageContinuousMode(showToast = false)
        }

        inputModule.close()
        imageCaptureSession.destroy()
        presetRepository.cancelExecution()
        presetRepository.resetState()
        pendingImageBytes = null
        pendingTextSelectInput = null
        imageContinuousRearmPending = false
        activePreset = resolved

        if (resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT) {
            // Gate: require accessibility service enabled. Show the prominent
            // disclosure first (Google Play requirement), then open Settings on consent.
            if (!SgtAccessibilityService.isAvailable) {
                promptAccessibilityDisclosure()
                return
            }

            // Capture selected text, then decide flow based on promptMode
            val svc = SgtAccessibilityService.instance
            val treeText = svc?.getSelectedText()
            if (!treeText.isNullOrBlank()) {
                executeTextSelectWithCapturedText(resolved, treeText)
            } else {
                // Click system "Copy" button to put selection into clipboard
                svc?.eagerCaptureSelection()
                processingIndicator.show(androidx.compose.ui.graphics.Color(0xFF5DB882))

                // Try reading clipboard via accessibility overlay (no visual artifact)
                svc?.readClipboardAsync { overlayResult ->
                    if (!overlayResult.isNullOrBlank()) {
                        processingIndicator.dismiss()
                        executeTextSelectWithCapturedText(resolved, overlayResult)
                    } else {
                        // Fallback: transparent Activity (brief visual flash)
                        dev.screengoated.toolbox.mobile.service.ClipboardReaderActivity.launch(context) { clipboardText ->
                            processingIndicator.dismiss()
                            executeTextSelectWithCapturedText(resolved, clipboardText)
                        }
                        android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                            if (processingIndicator.isShowing) {
                                processingIndicator.dismiss()
                                val lang = uiLanguage()
                                val msg = when (lang) {
                                    "vi" -> "Hãy copy text trước, sau đó bấm lại preset này"
                                    "ko" -> "먼저 텍스트를 복사한 후 이 프리셋을 다시 누르세요"
                                    else -> "Copy text first, then tap this preset again"
                                }
                                Toast.makeText(context, msg, Toast.LENGTH_LONG).show()
                            }
                        }, 5000)
                    }
                }
            }
        } else if (resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE) {
            launchImagePreset(resolved, continuousMode)
        } else if (
            resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC ||
                resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO
        ) {
            imageContinuousPresetId = null
            launchAudioPreset(resolved)
        } else {
            imageContinuousPresetId = null
            inputModule.open(resolved)
        }
        if (!closePanel) {
            // Panel doesn't overlap bubble — no z-reorder needed
        }
    }

    /** Captured image bytes from IMAGE preset, waiting for dynamic prompt input. */
    internal var pendingImageBytes: ByteArray? = null

    internal fun launchDefaultMicPreset() {
        val resolved = presetRepository.getResolvedPreset("preset_transcribe") ?: return
        if (!inputModule.hasWindow()) {
            // No input window — normal preset launch
            launchPreset(presetId = resolved.preset.id, closePanel = false, continuousMode = false)
            return
        }
        // Input window is open — mic is just a speech-to-text input method.
        // Record audio, transcribe, inject text into input. Do NOT run the preset pipeline.
        // (matches Windows: show_recording_overlay → set_editor_text, input window stays open)
        if (audioCaptureSession.toggleOrAbortIfMatching(resolved.preset.id)) return
        onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.MICROPHONE)
        audioCaptureSession.start(
            resolvedPreset = resolved,
            onRecordingComplete = { capture ->
                onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
                val transcript = capture.precomputedTranscript
                if (!transcript.isNullOrBlank()) {
                    // Streaming transcript is already available from the capture session.
                    inputModule.injectText(transcript)
                    inputModule.bringToFront()
                } else {
                    // Standard runtime (Whisper/Groq) — call transcription API directly
                    val audioBlock = resolved.preset.blocks.firstOrNull {
                        it.blockType == dev.screengoated.toolbox.mobile.shared.preset.BlockType.AUDIO
                    } ?: return@start
                    scope.launch(Dispatchers.Main) {
                        val result = appContainer.audioApiClient.executeStreaming(
                            modelId = audioBlock.model,
                            prompt = audioBlock.resolvePrompt(),
                            wavBytes = capture.wavBytes,
                            apiKeys = buildApiKeys(),
                            uiLanguage = uiLanguage(),
                            onChunk = {},
                        )
                        result.getOrNull()?.takeIf { it.isNotBlank() }?.let { text ->
                            inputModule.injectText(text)
                            inputModule.bringToFront()
                        }
                        result.exceptionOrNull()?.let { error ->
                            apiKeyErrorToastText(error.message ?: error.toString(), uiLanguage())
                                ?.let(appContainer.toastBus::show)
                        }
                    }
                }
            },
            onCancelled = {
                onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
            },
            onFailure = { failure ->
                onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
                handleAudioCaptureFailure(resolved, failure)
            },
        )
    }

    fun resumePendingAudioLaunch() {
        val pending = appContainer.audioPresetLaunchStore.take() ?: return
        if (pending.kind != AudioPresetLaunchKind.CAPTURE) {
            appContainer.audioPresetLaunchStore.set(pending)
            return
        }
        launchPreset(
            presetId = pending.presetId,
            closePanel = false,
            continuousMode = false,
        )
    }

    internal fun appendStreamingTextChunk(chunk: String): Boolean {
        if (chunk.isBlank()) {
            return false
        }
        val service = SgtAccessibilityService.instance ?: return false
        return service.appendTextToFocusedField(
            text = chunk,
            uiLanguage = uiLanguage(),
        )
    }

    /**
     * Handle TEXT_SELECT after the selected text has been captured.
     * Fixed prompt → execute immediately.
     * Dynamic prompt → show input window, user types prompt, then execute with modified preset.
     * Matches Windows pipeline.rs:299-358.
     */
    internal fun executeTextSelectWithCapturedText(resolved: ResolvedPreset, capturedText: String) {
        if (resolved.preset.promptMode == "dynamic") {
            // Dynamic: show input window for user to type the prompt
            // Store captured text for later — will combine with user's prompt on submit
            pendingTextSelectInput = capturedText
            inputModule.open(resolved)
        } else {
            // Fixed: execute immediately with preset's built-in prompt
            presetRepository.executePreset(resolved.preset, PresetInput.Text(capturedText))
        }
    }

    /** Captured text from TEXT_SELECT, waiting for dynamic prompt input. */
    internal var pendingTextSelectInput: String? = null

    internal fun handleInputClosedWithoutResults() {
        val hadPendingImage = pendingImageBytes != null
        pendingImageBytes = null
        pendingTextSelectInput = null
        if (hadPendingImage && imageContinuousPresetId != null) {
            val resolved = activePreset?.takeIf { it.preset.id == imageContinuousPresetId }
                ?: presetRepository.getResolvedPreset(imageContinuousPresetId!!)
            if (resolved != null) {
                startImageCaptureSession(
                    resolved = resolved,
                    continuousMode = true,
                    trace = newImageCaptureTrace(
                        resolved = resolved,
                        continuousMode = true,
                        source = "dynamic_input_cancel_rearm",
                    ),
                )
                return
            }
        }
        activePreset = null
    }

    internal fun submitInput(text: String) {
        val resolved = activePreset ?: return
        val pendingImage = pendingImageBytes
        if (pendingImage != null) {
            // IMAGE + dynamic prompt: inject user's prompt, execute with captured image
            pendingImageBytes = null
            val modifiedPreset = mutateDynamicPromptPreset(resolved, text)
            presetRepository.resetState()
            presetRepository.executePreset(modifiedPreset, PresetInput.Image(pendingImage))
            imageContinuousRearmPending = imageContinuousPresetId == resolved.preset.id
            inputModule.recordSubmittedText(text)
            return
        }
        val pending = pendingTextSelectInput
        if (pending != null) {
            // TEXT_SELECT + dynamic prompt: inject user's prompt into preset, execute with captured text
            // Matches Windows pipeline.rs:323-335
            pendingTextSelectInput = null
            val modifiedPreset = mutateDynamicPromptPreset(resolved, text)
            presetRepository.resetState()
            presetRepository.executePreset(modifiedPreset, PresetInput.Text(pending))
            inputModule.recordSubmittedText(text)
        } else {
            // Normal TEXT_INPUT flow
            presetRepository.resetState()
            presetRepository.executePreset(resolved.preset, PresetInput.Text(text))
            inputModule.recordSubmittedText(text)
        }
    }

    internal fun renderExecutionState(state: PresetExecutionState) {
        // Show processing indicator ONLY until the first result overlay appears.
        // Once any result window exists (even loading), dismiss the indicator —
        // the result overlay has its own loading animation inside.
        // (matches Windows: processing animation closes on first streaming chunk)
        val preset = activePreset?.preset
        val hasAnyResultWindow = state.resultWindows.any { window ->
            preset == null ||
                preset.blocks.getOrNull(window.blockIdx)?.blockType != dev.screengoated.toolbox.mobile.shared.preset.BlockType.INPUT_ADAPTER
        }
        if (state.isExecuting && !hasAnyResultWindow) {
            if (!processingIndicator.isShowing) {
                val accentColor = when (activePreset?.preset?.presetType) {
                    dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE ->
                        androidx.compose.ui.graphics.Color(0xFF5C9CE6)
                    dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT,
                    dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT ->
                        androidx.compose.ui.graphics.Color(0xFF5DB882)
                    dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC,
                    dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO ->
                        androidx.compose.ui.graphics.Color(0xFFDCA850)
                    else -> androidx.compose.ui.graphics.Color(0xFF5C9CE6)
                }
                processingIndicator.show(accentColor)
            }
        } else {
            processingIndicator.dismiss()
        }

        resultModule.renderExecutionState(state, activePreset)
        if (!state.isExecuting) {
            maybeRearmImageContinuous()
        }
    }

    internal fun favoritePresets(): List<ResolvedPreset> {
        return presetRepository.catalogState.value.presets.filter { it.preset.isFavorite }
    }

    internal fun favoritePanelPresets(): List<ResolvedPreset> {
        return favoritePresets().filter { !it.preset.isUpcoming }
    }

    internal fun screenBounds(): Rect {
        val metrics = context.resources.displayMetrics
        return Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
    }

    internal fun uiLanguage(): String = uiPreferencesProvider().uiLanguage

    internal fun localized(en: String, vi: String, ko: String): String =
        overlayLocalized(uiLanguage(), en, vi, ko)

    internal fun isDarkTheme(): Boolean = overlayIsDarkTheme(context, uiPreferencesProvider().themeMode)

    internal fun buildApiKeys(): dev.screengoated.toolbox.mobile.preset.ApiKeys {
        val repo = appContainer.repository
        return dev.screengoated.toolbox.mobile.preset.ApiKeys(
            geminiKey = repo.currentApiKey(),
            cerebrasKey = repo.currentCerebrasApiKey(),
            groqKey = repo.currentGroqApiKey(),
            openRouterKey = repo.currentOpenRouterApiKey(),
            ollamaBaseUrl = repo.currentOllamaUrl(),
        )
    }

    internal fun dp(value: Int): Int = (value * density).roundToInt()

    internal fun cssToPhysical(value: Float): Int = (value * density).roundToInt()

    internal fun cssToPhysical(value: Int): Int = cssToPhysical(value.toFloat())

    internal fun mutateDynamicPromptPreset(
        resolved: ResolvedPreset,
        userText: String,
    ): dev.screengoated.toolbox.mobile.shared.preset.Preset {
        val modifiedBlocks = resolved.preset.blocks.toMutableList()
        val targetIdx = modifiedBlocks.indexOfFirst {
            it.blockType != dev.screengoated.toolbox.mobile.shared.preset.BlockType.INPUT_ADAPTER
        }
        if (targetIdx >= 0) {
            val block = modifiedBlocks[targetIdx]
            modifiedBlocks[targetIdx] = block.copy(
                prompt = appendDynamicUserRequest(block.prompt, userText),
            )
        }
        return resolved.preset.copy(blocks = modifiedBlocks)
    }

    internal fun setOverlayChromeSuppressed(suppressed: Boolean) {
        panelModule.setSuppressed(suppressed)
        inputModule.setSuppressed(suppressed)
        resultModule.setSuppressed(suppressed)
    }

    internal fun dismissAllOverlays() {
        processingIndicator.dismiss()
        dismissTarget.hide()
        panelModule.dismiss()
        stopImageContinuousMode(showToast = false)
        if (audioCaptureSession.isActive) {
            audioCaptureSession.cancel()
        }
        onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
        inputModule.close()
        resultModule.resetExecution(resetRepository = true)
        pendingImageBytes = null
        pendingTextSelectInput = null
        activePreset = null
        setOverlayChromeSuppressed(false)
    }
}
