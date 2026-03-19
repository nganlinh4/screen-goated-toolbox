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

            // TEXT_SELECT: read selected text or clipboard, then execute immediately
            val svc = dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.instance
            android.util.Log.d("TextSelect", "Accessibility instance: ${svc != null}")
            android.util.Log.d("TextSelect", "isAvailable: ${dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.isAvailable}")
            if (svc != null) {
                val root = try { svc.rootInActiveWindow } catch (_: Exception) { null }
                android.util.Log.d("TextSelect", "rootInActiveWindow: ${root != null}, pkg=${root?.packageName}")
                val selectedDirect = svc.getSelectedText()
                android.util.Log.d("TextSelect", "getSelectedText(): '${selectedDirect}'")
                val clipText = svc.getClipboardText()
                android.util.Log.d("TextSelect", "getClipboardText(): '${clipText?.take(50)}'")
                root?.recycle()
            }
            val capturedText = dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
                .getSelectedTextOrClipboard(context)
            if (!capturedText.isNullOrBlank()) {
                // Got text — execute immediately, no editor
                presetRepository.executePreset(
                    resolved.preset,
                    dev.screengoated.toolbox.mobile.shared.preset.PresetInput.Text(capturedText),
                )
            } else {
                // No text selected and clipboard empty — tell user to select text first
                val lang = uiLanguage()
                val msg = when (lang) {
                    "vi" -> "Hãy bôi chọn text trước, sau đó bấm lại preset này"
                    "ko" -> "먼저 텍스트를 선택한 후 이 프리셋을 다시 누르세요"
                    else -> "Select text first, then tap this preset again"
                }
                Toast.makeText(context, msg, Toast.LENGTH_LONG).show()
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
