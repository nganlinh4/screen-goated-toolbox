package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Rect
import android.os.SystemClock
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.service.ScreenshotCaptureFailureReason
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

internal class PresetOverlayController(
    private val context: Context,
    private val scope: CoroutineScope,
    private val windowManager: WindowManager,
    private val presetRepository: PresetRepository,
    private val uiPreferencesFlow: StateFlow<MobileUiPreferences>,
    private val uiPreferencesProvider: () -> MobileUiPreferences,
    private val keepOpenProvider: () -> Boolean,
    private val onKeepOpenChanged: (Boolean) -> Unit,
    private val onIncreaseBubbleSize: () -> Unit,
    private val onDecreaseBubbleSize: () -> Unit,
    private val onPanelExpandedChanged: (Boolean) -> Unit = {},
    private val onBubbleSuppressedChanged: (Boolean) -> Unit = {},
    private val onRequestBubbleFront: () -> Unit = {},
    private val ttsRuntimeService: TtsRuntimeService? = null,
    private val ttsSettingsSnapshotProvider: (() -> dev.screengoated.toolbox.mobile.service.tts.TtsRequestSettingsSnapshot)? = null,
) {
    private val favoriteBubbleHtmlBuilder = FavoriteBubbleHtmlBuilder()
    private val textInputHtmlBuilder = PresetTextInputHtmlBuilder()
    private val density = context.resources.displayMetrics.density
    private val dismissTarget = PresetOverlayDismissTarget(context, windowManager)
    private val clipboardManager = context.getSystemService(ClipboardManager::class.java)

    private val processingIndicator = PresetProcessingIndicator(context, windowManager)
    private val imageCaptureSession = PresetImageCaptureSession(
        context = context,
        windowManager = windowManager,
        uiLanguage = ::uiLanguage,
        onBubbleSuppressedChanged = onBubbleSuppressedChanged,
        onOverlaySuppressedChanged = ::setOverlayChromeSuppressed,
    )
    private var activePreset: ResolvedPreset? = null
    private var bubbleBounds = OverlayBounds(x = 0, y = 0, width = dp(48), height = dp(48))
    private var imageContinuousPresetId: String? = null
    private var imageContinuousRearmPending = false
    private var nextImageCaptureTraceId = 1L

    private val panelModule = PresetOverlayPanelModule(
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
    private lateinit var inputModule: PresetOverlayInputModule
    private lateinit var resultModule: PresetOverlayResultModule

    private var catalogJob: Job? = null
    private var executionJob: Job? = null
    private var uiPreferencesJob: Job? = null
    private var ttsEventsJob: Job? = null
    private var lastUiPreferences: MobileUiPreferences = uiPreferencesProvider()

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
            onNoOverlaysRemaining = {
                if (!::inputModule.isInitialized || !inputModule.hasWindow()) {
                    activePreset = null
                }
            },
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
            onInputClosedWithoutResults = ::handleInputClosedWithoutResults,
            hasResults = { resultModule.hasResults() },
        )
        // Wire centralized post-processing actions (matches Windows step.rs)
        presetRepository.postProcessActions = object : dev.screengoated.toolbox.mobile.preset.PresetPostProcessActions {
            override fun handleAutoCopy(block: dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock, resultText: String) {
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    val svc = SgtAccessibilityService.instance
                    svc?.copyToClipboard(resultText)
                }
            }

            override fun handleAutoCopyImage(
                block: dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock,
                pngBytes: ByteArray,
            ) {
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    val copied = SgtAccessibilityService.instance?.copyImageToClipboard(pngBytes) == true
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
                android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                    val tts = ttsRuntimeService ?: return@postDelayed
                    val snapshot = ttsSettingsSnapshotProvider?.invoke() ?: return@postDelayed
                    tts.enqueue(
                        dev.screengoated.toolbox.mobile.service.tts.TtsRequest(
                            text = resultText,
                            consumer = dev.screengoated.toolbox.mobile.service.tts.TtsConsumer.RESULT_OVERLAY,
                            priority = dev.screengoated.toolbox.mobile.service.tts.TtsPriority.USER,
                            requestMode = dev.screengoated.toolbox.mobile.service.tts.TtsRequestMode.NORMAL,
                            settingsSnapshot = snapshot,
                            ownerToken = "autospeak_block_$blockIdx",
                        ),
                    )
                }, 200)
            }

            override fun handleAutoPaste() {
                android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                    val svc = SgtAccessibilityService.instance
                    svc?.pasteIntoFocusedField()
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
        inputModule.destroy()
        resultModule.destroy()
        dismissTarget.hide()
        activePreset = null
        imageContinuousPresetId = null
        imageContinuousRearmPending = false
        pendingImageBytes = null
        pendingTextSelectInput = null
        presetRepository.resetState()
    }

    private fun refreshOverlayPreferences(
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

        if (themeChanged) {
            resultModule.refreshResultWindowsForTheme()
        }
        if (themeChanged || languageChanged) {
            resultModule.refreshCanvasWindowForPreferences()
        }
    }

    private fun launchPreset(
        presetId: String,
        closePanel: Boolean,
        continuousMode: Boolean,
    ) {
        if (closePanel) {
            panelModule.dismiss()
        }
        val resolved = presetRepository.getResolvedPreset(presetId) ?: return
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
            // Gate: require accessibility service enabled
            if (!SgtAccessibilityService.isAvailable) {
                val lang = uiLanguage()
                val msg = when (lang) {
                    "vi" -> "Cần bật Dịch vụ trợ năng để dùng preset này. Đang mở Cài đặt..."
                    "ko" -> "이 프리셋을 사용하려면 접근성 서비스를 활성화해야 합니다. 설정을 여는 중..."
                    else -> "Accessibility service required for this preset. Opening Settings..."
                }
                Toast.makeText(context, msg, Toast.LENGTH_LONG).show()
                try {
                    val intent = android.content.Intent(android.provider.Settings.ACTION_ACCESSIBILITY_SETTINGS)
                    intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                    context.startActivity(intent)
                } catch (_: Exception) {}
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
        } else {
            imageContinuousPresetId = null
            inputModule.open(resolved)
        }
        if (!closePanel) {
            // Panel doesn't overlap bubble — no z-reorder needed
        }
    }

    /** Captured image bytes from IMAGE preset, waiting for dynamic prompt input. */
    private var pendingImageBytes: ByteArray? = null

    private fun launchImagePreset(
        resolved: ResolvedPreset,
        continuousMode: Boolean,
    ) {
        val trace = newImageCaptureTrace(
            resolved = resolved,
            continuousMode = continuousMode,
            source = "preset_press",
        )
        logImageCaptureTrace(trace, "preset_pressed")
        if (continuousMode && imageContinuousPresetId == resolved.preset.id) {
            stopImageContinuousMode(showToast = true)
            return
        }

        imageContinuousPresetId = if (continuousMode) resolved.preset.id else null
        imageContinuousRearmPending = false
        if (continuousMode) {
            Toast.makeText(
                context,
                localized(
                    "Image continuous mode armed.",
                    "Đã bật chế độ chụp ảnh liên tục.",
                    "이미지 연속 모드가 활성화되었습니다.",
                ),
                Toast.LENGTH_SHORT,
            ).show()
        }
        startImageCaptureSession(resolved, continuousMode, trace)
    }

    /**
     * Handle TEXT_SELECT after the selected text has been captured.
     * Fixed prompt → execute immediately.
     * Dynamic prompt → show input window, user types prompt, then execute with modified preset.
     * Matches Windows pipeline.rs:299-358.
     */
    private fun executeTextSelectWithCapturedText(resolved: ResolvedPreset, capturedText: String) {
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
    private var pendingTextSelectInput: String? = null

    private fun handleInputClosedWithoutResults() {
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

    private fun submitInput(text: String) {
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

    private fun renderExecutionState(state: PresetExecutionState) {
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

    private fun favoritePresets(): List<ResolvedPreset> {
        return presetRepository.catalogState.value.presets.filter { it.preset.isFavorite }
    }

    private fun favoritePanelPresets(): List<ResolvedPreset> {
        return favoritePresets().filter { !it.preset.isUpcoming }
    }

    private fun screenBounds(): Rect {
        val metrics = context.resources.displayMetrics
        return Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
    }

    private fun uiLanguage(): String = uiPreferencesProvider().uiLanguage

    private fun localized(en: String, vi: String, ko: String): String =
        overlayLocalized(uiLanguage(), en, vi, ko)

    private fun isDarkTheme(): Boolean = overlayIsDarkTheme(context, uiPreferencesProvider().themeMode)

    private fun dp(value: Int): Int = (value * density).roundToInt()

    private fun cssToPhysical(value: Float): Int = (value * density).roundToInt()

    private fun cssToPhysical(value: Int): Int = cssToPhysical(value.toFloat())

    private fun startImageCaptureSession(
        resolved: ResolvedPreset,
        continuousMode: Boolean,
        trace: ImageCaptureTrace,
    ) {
        processingIndicator.dismiss()
        imageCaptureSession.start(
            resolvedPreset = resolved,
            trace = trace,
            onSelectionConfirmed = { pngBytes ->
                if (resolved.preset.promptMode == "dynamic") {
                    pendingImageBytes = pngBytes
                    inputModule.open(resolved)
                } else {
                    presetRepository.executePreset(resolved.preset, PresetInput.Image(pngBytes))
                    imageContinuousRearmPending = continuousMode
                }
            },
            onColorPicked = { hexColor ->
                copyColorToClipboard(hexColor)
                if (continuousMode && imageContinuousPresetId == resolved.preset.id) {
                    startImageCaptureSession(
                        resolved = resolved,
                        continuousMode = true,
                        trace = newImageCaptureTrace(
                            resolved = resolved,
                            continuousMode = true,
                            source = "color_pick_rearm",
                        ),
                    )
                } else {
                    activePreset = null
                }
            },
            onCancelled = {
                pendingImageBytes = null
                imageContinuousRearmPending = false
                if (continuousMode) {
                    stopImageContinuousMode(showToast = false)
                } else {
                    activePreset = null
                }
            },
            onCaptureFailure = { reason ->
                imageContinuousRearmPending = false
                handleImageCaptureFailure(reason, continuousMode)
            },
        )
    }

    private fun maybeRearmImageContinuous() {
        val presetId = imageContinuousPresetId ?: return
        if (!imageContinuousRearmPending || imageCaptureSession.isActive || inputModule.hasWindow()) {
            return
        }
        val resolved = presetRepository.getResolvedPreset(presetId) ?: return
        imageContinuousRearmPending = false
        startImageCaptureSession(
            resolved = resolved,
            continuousMode = true,
            trace = newImageCaptureTrace(
                resolved = resolved,
                continuousMode = true,
                source = "continuous_rearm",
            ),
        )
    }

    private fun stopImageContinuousMode(showToast: Boolean) {
        val wasActive = imageContinuousPresetId != null
        imageContinuousPresetId = null
        imageContinuousRearmPending = false
        pendingImageBytes = null
        imageCaptureSession.destroy()
        if (showToast && wasActive) {
            Toast.makeText(
                context,
                localized(
                    "Image continuous mode exited.",
                    "Đã thoát chế độ chụp ảnh liên tục.",
                    "이미지 연속 모드를 종료했습니다.",
                ),
                Toast.LENGTH_SHORT,
            ).show()
        }
    }

    private fun handleImageCaptureFailure(
        reason: ScreenshotCaptureFailureReason,
        continuousMode: Boolean,
    ) {
        if (continuousMode) {
            stopImageContinuousMode(showToast = false)
        }
        val message = when (reason) {
            ScreenshotCaptureFailureReason.API_TOO_OLD ->
                localized(
                    "Image presets require Android 11 or later.",
                    "Preset ảnh cần Android 11 trở lên.",
                    "이미지 프리셋은 Android 11 이상이 필요합니다.",
                )

            ScreenshotCaptureFailureReason.SERVICE_UNAVAILABLE,
            ScreenshotCaptureFailureReason.CAPABILITY_MISSING,
            ScreenshotCaptureFailureReason.NO_ACCESSIBILITY_ACCESS,
            ScreenshotCaptureFailureReason.SECURITY_EXCEPTION,
            -> localized(
                "Accessibility screenshot permission is required. Opening Settings...",
                "Cần quyền chụp màn hình của Dịch vụ trợ năng. Đang mở Cài đặt...",
                "접근성 스크린샷 권한이 필요합니다. 설정을 여는 중...",
            )

            ScreenshotCaptureFailureReason.RATE_LIMITED ->
                localized(
                    "Screenshot requested too quickly. Try again in a moment.",
                    "Yêu cầu chụp quá nhanh. Hãy thử lại sau một lát.",
                    "스크린샷 요청이 너무 빠릅니다. 잠시 후 다시 시도하세요.",
                )

            ScreenshotCaptureFailureReason.INVALID_TARGET ->
                localized(
                    "Could not capture this screen.",
                    "Không thể chụp màn hình này.",
                    "이 화면을 캡처할 수 없습니다.",
                )

            ScreenshotCaptureFailureReason.SECURE_WINDOW ->
                localized(
                    "This screen blocks screenshots.",
                    "Màn hình này chặn chụp màn hình.",
                    "이 화면은 스크린샷을 차단합니다.",
                )

            ScreenshotCaptureFailureReason.REQUEST_FAILED ->
                localized(
                    "Could not capture screenshot.",
                    "Không thể chụp màn hình.",
                    "스크린샷을 캡처할 수 없습니다.",
                )
        }
        Toast.makeText(
            context,
            message,
            if (reason.opensAccessibilitySettings()) Toast.LENGTH_LONG else Toast.LENGTH_SHORT,
        ).show()
        if (reason.opensAccessibilitySettings()) {
            openAccessibilitySettings()
        }
    }

    private fun mutateDynamicPromptPreset(
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

    private fun copyColorToClipboard(hexColor: String) {
        clipboardManager?.setPrimaryClip(ClipData.newPlainText("SGT Color", hexColor))
        Toast.makeText(
            context,
            localized(
                "Copied $hexColor",
                "Đã sao chép $hexColor",
                "$hexColor 복사됨",
            ),
            Toast.LENGTH_SHORT,
        ).show()
    }

    private fun setOverlayChromeSuppressed(suppressed: Boolean) {
        panelModule.setSuppressed(suppressed)
        inputModule.setSuppressed(suppressed)
        resultModule.setSuppressed(suppressed)
    }

    private fun openAccessibilitySettings() {
        try {
            val intent = android.content.Intent(android.provider.Settings.ACTION_ACCESSIBILITY_SETTINGS)
            intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(intent)
        } catch (_: Exception) {
        }
    }

    private fun ScreenshotCaptureFailureReason.opensAccessibilitySettings(): Boolean {
        return this == ScreenshotCaptureFailureReason.SERVICE_UNAVAILABLE ||
            this == ScreenshotCaptureFailureReason.CAPABILITY_MISSING ||
            this == ScreenshotCaptureFailureReason.NO_ACCESSIBILITY_ACCESS ||
            this == ScreenshotCaptureFailureReason.SECURITY_EXCEPTION
    }

    private fun newImageCaptureTrace(
        resolved: ResolvedPreset,
        continuousMode: Boolean,
        source: String,
    ): ImageCaptureTrace {
        return ImageCaptureTrace(
            id = nextImageCaptureTraceId++,
            presetId = resolved.preset.id,
            startedAtMs = SystemClock.elapsedRealtime(),
            continuousMode = continuousMode,
            source = source,
        )
    }
}
