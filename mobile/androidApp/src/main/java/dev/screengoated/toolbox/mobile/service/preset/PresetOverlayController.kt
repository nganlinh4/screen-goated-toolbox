package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.Context
import android.graphics.Rect
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
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
    private val onRequestBubbleFront: () -> Unit = {},
    private val ttsRuntimeService: TtsRuntimeService? = null,
    private val ttsSettingsSnapshotProvider: (() -> dev.screengoated.toolbox.mobile.service.tts.TtsRequestSettingsSnapshot)? = null,
) {
    private val favoriteBubbleHtmlBuilder = FavoriteBubbleHtmlBuilder()
    private val textInputHtmlBuilder = PresetTextInputHtmlBuilder()
    private val density = context.resources.displayMetrics.density
    private val dismissTarget = PresetOverlayDismissTarget(context, windowManager)

    private val processingIndicator = PresetProcessingIndicator(context, windowManager)
    private var activePreset: ResolvedPreset? = null
    private var bubbleBounds = OverlayBounds(x = 0, y = 0, width = dp(48), height = dp(48))

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
            onInputClosedWithoutResults = { activePreset = null },
            hasResults = { resultModule.hasResults() },
        )
        // Wire centralized post-processing actions (matches Windows step.rs)
        presetRepository.postProcessActions = object : dev.screengoated.toolbox.mobile.preset.PresetPostProcessActions {
            override fun handleAutoCopy(block: dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock, resultText: String) {
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    val svc = dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.instance
                    svc?.copyToClipboard(resultText)
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
                    val svc = dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.instance
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
        inputModule.destroy()
        resultModule.destroy()
        dismissTarget.hide()
        activePreset = null
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

    private fun launchPreset(presetId: String) {
        launchPreset(presetId, closePanel = true)
    }

    private fun launchPreset(
        presetId: String,
        closePanel: Boolean,
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

        inputModule.close()
        presetRepository.cancelExecution()
        presetRepository.resetState()
        activePreset = resolved

        if (resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT) {
            // Gate: require accessibility service enabled
            if (!dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.isAvailable) {
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

            // 1. Try accessibility tree scan first (instant, works for EditText)
            val svc = dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.instance
            val treeText = svc?.getSelectedText()
            if (!treeText.isNullOrBlank()) {
                android.util.Log.d("TextSelect", "Got text from tree scan: ${treeText.take(50)}")
                presetRepository.executePreset(resolved.preset, PresetInput.Text(treeText))
            } else {
                // 2. Click system "Copy" button to capture current selection into clipboard
                //    MUST happen before ClipboardReaderActivity steals focus and dismisses toolbar
                svc?.eagerCaptureSelection()

                // 3. Async: launch ClipboardReaderActivity to read clipboard
                //    Can't block main thread — Activity needs main looper to gain focus
                android.util.Log.d("TextSelect", "Launching ClipboardReaderActivity")
                processingIndicator.show(androidx.compose.ui.graphics.Color(0xFF5DB882))
                dev.screengoated.toolbox.mobile.service.ClipboardReaderActivity.launch(context) { clipboardText ->
                    android.util.Log.d("TextSelect", "ClipboardReaderActivity callback: '${clipboardText.take(50)}'")
                    processingIndicator.dismiss()
                    presetRepository.executePreset(resolved.preset, PresetInput.Text(clipboardText))
                }
                // If Activity fails to read clipboard within 5s, show fallback message
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
        } else {
            inputModule.open(resolved)
        }
        if (!closePanel) {
            onRequestBubbleFront()
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
    }

    private fun submitInput(text: String) {
        val resolved = activePreset ?: return
        presetRepository.resetState()
        presetRepository.executePreset(resolved.preset, PresetInput.Text(text))
        inputModule.recordSubmittedText(text)
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
}
