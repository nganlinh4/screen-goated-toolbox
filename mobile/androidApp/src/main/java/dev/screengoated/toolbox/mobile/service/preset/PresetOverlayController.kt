package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.res.Configuration
import android.graphics.Rect
import android.util.Log
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.Toast
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.util.LinkedHashMap
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
) {
    private val clipboardManager = context.getSystemService(ClipboardManager::class.java)
    private val favoriteBubbleHtmlBuilder = FavoriteBubbleHtmlBuilder()
    private val textInputHtmlBuilder = PresetTextInputHtmlBuilder()
    private val resultHtmlBuilder = PresetResultHtmlBuilder(context)
    private val buttonCanvasHtmlBuilder = PresetButtonCanvasHtmlBuilder(context)
    private val renderer = PresetMarkdownRenderer(context)
    private val density = context.resources.displayMetrics.density

    private var panelWindow: PresetOverlayWindow? = null
    private var inputWindow: PresetOverlayWindow? = null
    private var canvasWindow: PresetOverlayWindow? = null
    private val resultWindows = LinkedHashMap<PresetResultWindowId, ActivePresetResultWindow>()

    private var activePreset: ResolvedPreset? = null
    private var activeResultWindowId: PresetResultWindowId? = null
    private var bubbleBounds = OverlayBounds(x = 0, y = 0, width = dp(48), height = dp(48))
    private var panelPresetIds: List<String> = emptyList()
    private var panelClosing = false
    private var inputClosing = false
    private val inputHistory = mutableListOf<String>()
    private var historyNavigationIndex: Int? = null
    private var historyDraftText: String = ""
    private var dismissBubbleView: View? = null
    private var lastFingerDistSq = Int.MAX_VALUE

    private var catalogJob: Job? = null
    private var executionJob: Job? = null
    private var uiPreferencesJob: Job? = null
    private var lastUiPreferences: MobileUiPreferences = uiPreferencesProvider()

    init {
        catalogJob = scope.launch(Dispatchers.Main.immediate) {
            presetRepository.catalogState.collectLatest {
                if (panelWindow != null) {
                    renderPanel(animate = false)
                }
                val activeId = activePreset?.preset?.id
                if (activeId != null) {
                activePreset = presetRepository.getResolvedPreset(activeId)
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
                if (previous == preferences) {
                    return@collectLatest
                }
                refreshOverlayPreferences(previous, preferences)
            }
        }
    }

    fun updateBubbleBounds(bounds: OverlayBounds) {
        bubbleBounds = bounds
        val window = panelWindow ?: return
        val panelSpec = panelWindowSpec(panelPresetIds.size)
        window.updateBounds(
            OverlayBounds(
                x = panelSpec.x,
                y = panelSpec.y,
                width = panelSpec.width,
                height = window.currentBounds().height,
            ),
        )
        syncPanelWindowState(window)
    }

    fun togglePanel() {
        runCatching {
            if (panelWindow != null) {
                closePanel(animate = true)
            } else {
                openPanel()
            }
        }.onFailure {
            closePanel(animate = false)
            Toast.makeText(
                context,
                localized("Bubble panel is not ready yet.", "Bảng điều khiển bong bóng chưa sẵn sàng.", "버블 패널이 아직 준비되지 않았습니다."),
                Toast.LENGTH_SHORT,
            ).show()
        }
    }

    fun dismissPanel() {
        closePanel(animate = false)
    }

    fun destroy() {
        catalogJob?.cancel()
        executionJob?.cancel()
        uiPreferencesJob?.cancel()
        onPanelExpandedChanged(false)
        panelWindow?.destroy()
        inputWindow?.destroy()
        canvasWindow?.destroy()
        resultWindows.values.forEach { it.window.destroy() }
        hideDismissZone()
        panelWindow = null
        inputWindow = null
        canvasWindow = null
        resultWindows.clear()
        activePreset = null
        activeResultWindowId = null
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

        if (panelWindow != null) {
            renderPanel(animate = false)
        }

        if (inputWindow != null) {
            refreshInputWindowForPreferences()
        }

        if (themeChanged) {
            refreshResultWindowsForTheme()
        }
        if (themeChanged || languageChanged) {
            refreshCanvasWindowForTheme()
        } else if (canvasWindow != null) {
            ensureCanvasWindow()
        }
    }

    private fun refreshInputWindowForPreferences() {
        val window = inputWindow ?: return
        val preset = activePreset ?: return
        window.runScriptForResult("window.exportDraftState && window.exportDraftState();") { raw ->
            val draftState = parseDraftState(raw)
            window.loadHtmlContent(buildInputHtml(preset), INPUT_WINDOW_BASE_URL)
            if (draftState.isNotEmpty()) {
                window.runScript(
                    "window.restoreDraftState(${JSONObject.quote(JSONObject().put("text", draftState).toString())});",
                )
            }
            window.runScript("window.focusEditor && window.focusEditor();")
        }
    }

    private fun refreshResultWindowsForTheme() {
        val isDark = isDarkTheme(uiPreferencesProvider().themeMode)
        resultWindows.values.forEach { active ->
            if (active.runtimeState.isBrowsing || active.runtimeState.isRawHtml) {
                return@forEach
            }
            active.window.loadHtmlContent(
                resultHtmlBuilder.build(
                    PresetResultHtmlSettings(
                        isDark = isDark,
                    ),
                ),
                RESULT_WINDOW_BASE_URL,
            )
            updateResultWindow(active)
        }
    }

    private fun refreshCanvasWindowForTheme() {
        val window = canvasWindow ?: return
        window.loadHtmlContent(
            buttonCanvasHtmlBuilder.build(
                PresetButtonCanvasHtmlSettings(
                    lang = uiLanguage(),
                    isDark = isDarkTheme(uiPreferencesProvider().themeMode),
                ),
            ),
            CANVAS_WINDOW_BASE_URL,
        )
        ensureCanvasWindow()
    }

    private fun parseDraftState(raw: String?): String {
        if (raw.isNullOrBlank() || raw == "null") {
            return ""
        }
        val json = raw.removeSurrounding("\"").replace("\\\\", "\\").replace("\\\"", "\"")
        return json.jsonOrNull()?.optString("text").orEmpty()
    }

    private fun openPanel() {
        val favorites = favoritePanelPresets()
        if (favorites.isEmpty()) {
            Toast.makeText(context, emptyFavoritesMessage(uiLanguage()), Toast.LENGTH_SHORT).show()
            return
        }
        panelClosing = false
        val spec = panelWindowSpec(favorites.size)
        panelWindow = PresetOverlayWindow(
            context = context,
            windowManager = windowManager,
            spec = spec.copy(
                htmlContent = buildPanelHtml(favorites),
                baseUrl = FAVORITE_PANEL_BASE_URL,
                clipToOutline = false,
            ),
            onMessage = ::handlePanelMessage,
        ).also { window ->
            window.show()
            onPanelExpandedChanged(true)
            syncPanelWindowState(window)
            window.runScript(openPanelScript(window))
            onRequestBubbleFront()
        }
    }

    private fun renderPanel(animate: Boolean) {
        val window = panelWindow ?: return
        val favorites = favoritePanelPresets()
        if (favorites.isEmpty()) {
            closePanel(animate = false)
            Toast.makeText(context, emptyFavoritesMessage(uiLanguage()), Toast.LENGTH_SHORT).show()
            return
        }
        panelPresetIds = favorites.map { it.preset.id }
        val panelSpec = panelWindowSpec(favorites.size)
        window.updateBounds(
            OverlayBounds(
                x = panelSpec.x,
                y = panelSpec.y,
                width = panelSpec.width,
                height = window.currentBounds().height.coerceAtLeast(minPanelHeight(favorites.size)),
            ),
        )
        window.loadHtmlContent(
            buildPanelHtml(favorites),
            FAVORITE_PANEL_BASE_URL,
        )
        syncPanelWindowState(window)
        window.runScript(if (animate) openPanelScript(window) else showPanelImmediatelyScript())
        onRequestBubbleFront()
    }

    private fun closePanel(animate: Boolean) {
        val window = panelWindow ?: return
        if (!animate) {
            panelClosing = false
            panelPresetIds = emptyList()
            onPanelExpandedChanged(false)
            window.destroy()
            panelWindow = null
            return
        }
        if (panelClosing) {
            return
        }
        panelClosing = true
        syncPanelWindowState(window)
        window.runScript("window.closePanel();")
    }

    private fun launchPreset(presetId: String) {
        launchPreset(presetId, closePanel = true)
    }

    private fun launchPreset(
        presetId: String,
        closePanel: Boolean,
    ) {
        if (closePanel) {
            closePanel(animate = false)
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

        closeInputWindow()
        closeAllResults(resetExecution = false)
        presetRepository.resetState()
        activePreset = resolved
        activeResultWindowId = null
        openInputWindow(resolved)
        if (!closePanel) {
            onRequestBubbleFront()
        }
    }

    private fun openInputWindow(resolvedPreset: ResolvedPreset) {
        closeInputWindow()
        inputClosing = false
        historyNavigationIndex = null
        historyDraftText = ""
        val spec = inputWindowSpec(buildInputHtml(resolvedPreset))
        inputWindow = PresetOverlayWindow(
            context = context,
            windowManager = windowManager,
            spec = spec,
            onMessage = ::handleInputMessage,
        ).also { window ->
            window.show()
            window.runScript("window.playEntry(); window.focusEditor();")
        }
    }

    private fun closeInputWindow() {
        hideDismissZone()
        inputClosing = false
        inputWindow?.destroy()
        inputWindow = null
    }

    private fun requestCloseInputWindow(animate: Boolean) {
        val window = inputWindow ?: return
        if (!animate) {
            finalizeInputWindowClose()
            return
        }
        if (inputClosing) {
            return
        }
        inputClosing = true
        window.runScript("window.closeWithAnimation();")
    }

    private fun finalizeInputWindowClose() {
        hideDismissZone()
        inputClosing = false
        inputWindow?.destroy()
        inputWindow = null
    }

    private fun ensureCanvasWindow() {
        val active = activeResultWindowId?.let(resultWindows::get)
        if (resultWindows.isEmpty() || active == null) {
            canvasWindow?.destroy()
            canvasWindow = null
            return
        }
        val layout = canvasWindowLayout(active)
        val spec = canvasWindowSpec(layout.bounds)
        if (canvasWindow == null) {
            canvasWindow = PresetOverlayWindow(
                context = context,
                windowManager = windowManager,
                spec = spec,
                onMessage = ::handleCanvasMessage,
            ).also { window ->
                window.show()
                window.loadHtmlContent(
                    buttonCanvasHtmlBuilder.build(
                        PresetButtonCanvasHtmlSettings(
                            lang = uiLanguage(),
                            isDark = isDarkTheme(uiPreferencesProvider().themeMode),
                        ),
                    ),
                    CANVAS_WINDOW_BASE_URL,
                )
            }
        } else {
            canvasWindow?.updateBounds(
                OverlayBounds(
                    x = spec.x,
                    y = spec.y,
                    width = spec.width,
                    height = spec.height,
                ),
            )
            canvasWindow?.show()
        }
        syncCanvasWindow(active = active, layout = layout, lingerMs = CANVAS_LINGER_MS)
    }

    private fun closeAllResults(resetExecution: Boolean = true) {
        presetRepository.cancelExecution()
        resultWindows.values.forEach { it.window.destroy() }
        resultWindows.clear()
        canvasWindow?.destroy()
        canvasWindow = null
        activeResultWindowId = null
        if (resetExecution) {
            presetRepository.resetState()
        }
    }

    private fun renderExecutionState(state: PresetExecutionState) {
        val activeId = activePreset?.preset?.id ?: return
        if (state.activePresetId != null && state.activePresetId != activeId) {
            return
        }

        val windowsToRender = when {
            state.error != null -> {
                val sessionId = state.sessionId ?: "preset-error-$activeId"
                listOf(
                    PresetResultWindowState(
                        id = PresetResultWindowId(sessionId = sessionId, blockIdx = 0),
                        blockIdx = 0,
                        title = activePreset?.preset?.name(uiLanguage()).orEmpty(),
                        markdownText = state.error,
                        isStreaming = false,
                        isError = true,
                        renderMode = "markdown",
                        overlayOrder = 0,
                    ),
                )
            }
            else -> state.resultWindows
        }

        if (windowsToRender.isEmpty()) {
            if (!state.isExecuting) {
                ensureCanvasWindow()
            }
            return
        }

        syncResultWindows(windowsToRender)
        ensureCanvasWindow()
    }

    private fun syncResultWindows(windowStates: List<PresetResultWindowState>) {
        val resolvedPreset = activePreset ?: return
        val targetIds = windowStates.map { it.id }.toSet()
        resultWindows.keys
            .filterNot(targetIds::contains)
            .toList()
            .forEach { id ->
                resultWindows.remove(id)?.window?.destroy()
                if (activeResultWindowId == id) {
                    activeResultWindowId = null
                }
            }

        val sortedStates = windowStates.sortedBy { it.overlayOrder }
        val placed = mutableListOf<PresetResultWindowPlacement>()
        sortedStates.forEach { windowState ->
            val existing = resultWindows[windowState.id]
            val runtime = existing?.runtimeState ?: PresetResultWindowRuntimeState(
                disabledActions = disabledActionsForWindow(),
            )
            val window = existing?.window ?: PresetOverlayWindow(
                context = context,
                windowManager = windowManager,
                spec = resultWindowSpec(resolvedPreset, windowState, placed),
                onMessage = ::handleResultMessage,
            ).also { it.show() }
            val bounds = window.currentBounds()
            placed += PresetResultWindowPlacement(windowState.id, bounds)
            val active = ActivePresetResultWindow(
                id = windowState.id,
                runtimeState = runtime,
                windowState = windowState,
                window = window,
            )
            resultWindows[windowState.id] = active
            updateResultWindow(active.copy(windowState = windowState))
        }
    }

    private fun handlePanelMessage(message: String) {
        when {
            message == "dismiss" || message == "close_now" -> closePanel(animate = false)
            message == "focus_bubble" -> onRequestBubbleFront()
            message == "panel_ready" -> {
                onRequestBubbleFront()
            }
            message.startsWith("resize:") -> {
                val window = panelWindow ?: return
                val measuredHeight = message.substringAfter("resize:", "").toIntOrNull() ?: return
                val clampedHeight = measuredHeight
                    .coerceAtLeast(minPanelHeight(panelPresetIds.size))
                    .coerceAtMost((screenBounds().height() * 0.62f).roundToInt())
                if (clampedHeight != window.currentBounds().height) {
                    window.updateBounds(window.currentBounds().copy(height = clampedHeight))
                }
            }
            message.startsWith("trigger:") ||
                message.startsWith("trigger_only:") ||
                message.startsWith("trigger_continuous:") ||
                message.startsWith("trigger_continuous_only:") -> {
                val index = message.substringAfter(':').toIntOrNull() ?: return
                val presetId = panelPresetIds.getOrNull(index) ?: return
                launchPreset(presetId, closePanel = false)
                if (
                    message.startsWith("trigger_only:") ||
                    message.startsWith("trigger_continuous_only:")
                ) {
                    onRequestBubbleFront()
                }
            }
            message.startsWith("set_keep_open:") -> {
                val enabled = message.substringAfter("set_keep_open:", "") == "1"
                onKeepOpenChanged(enabled)
                onRequestBubbleFront()
            }
            message == "increase_size" -> {
                onIncreaseBubbleSize()
                onRequestBubbleFront()
            }
            message == "decrease_size" -> {
                onDecreaseBubbleSize()
                onRequestBubbleFront()
            }
            message.startsWith("{") -> {
                val payload = message.jsonOrNull() ?: return
                when (payload.optString("type")) {
                    "closePanel" -> closePanel(animate = true)
                    "launchPreset" -> launchPreset(payload.optString("presetId"))
                    "panelRendered" -> Unit
                    "showUnsupported" -> {
                        val presetId = payload.optString("presetId")
                        val reason = presetRepository.getResolvedPreset(presetId)
                            ?.executionCapability
                            ?.reason
                        if (reason != null) {
                            Toast.makeText(context, placeholderReasonLabel(reason, uiLanguage()), Toast.LENGTH_SHORT).show()
                        }
                    }
                }
            }
        }
    }

    private fun handleInputMessage(message: String) {
        when {
            message == "close_window" || message == "cancel" -> {
                requestCloseInputWindow(animate = true)
                if (resultWindows.isEmpty()) {
                    activePreset = null
                }
                historyNavigationIndex = null
                historyDraftText = ""
            }
            message == "input_exit_done" -> {
                finalizeInputWindowClose()
            }
            message.startsWith("dragAt:") -> {
                updateDismissZone(fingerBubbleProximity(message.removePrefix("dragAt:")))
            }
            message.startsWith("dragEnd:") -> {
                val proximity = fingerBubbleProximity(message.removePrefix("dragEnd:"))
                lastFingerDistSq = Int.MAX_VALUE
                if (proximity >= 0.8f) {
                    hideDismissZone()
                    closeInputWindow()
                    if (resultWindows.isEmpty()) {
                        activePreset = null
                    }
                    historyNavigationIndex = null
                    historyDraftText = ""
                } else {
                    hideDismissZone()
                }
            }
            message == "mic" -> {
                Toast.makeText(
                    context,
                    placeholderReasonLabel(PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY, uiLanguage()),
                    Toast.LENGTH_SHORT,
                ).show()
            }
            message.startsWith("submit:") -> {
                val text = message.substringAfter("submit:", "").trim()
                if (text.isNotEmpty()) {
                    submitInput(text)
                }
            }
            message.startsWith("history_up:") -> {
                val current = message.substringAfter("history_up:", "")
                navigateInputHistory(current = current, upwards = true)
            }
            message.startsWith("history_down:") -> {
                val current = message.substringAfter("history_down:", "")
                navigateInputHistory(current = current, upwards = false)
            }
            message.startsWith("{") -> {
                val payload = message.jsonOrNull() ?: return
                when (payload.optString("type")) {
                    "dragInputWindow" -> {
                        ensureDismissBubble()
                        inputWindow?.moveBy(
                            dx = payload.optDouble("dx", 0.0).roundToInt(),
                            dy = payload.optDouble("dy", 0.0).roundToInt(),
                            screenBounds = screenBounds(),
                        )
                    }
                }
            }
        }
    }

    private fun handleResultMessage(message: String) {
        val payload = message.jsonOrNull() ?: return
        when (payload.optString("type")) {
            "gestureDebug" -> {
                Log.d(
                    TAG,
                    buildString {
                        append("resultGesture ")
                        append(payload.optString("phase"))
                        append(" window=")
                        append(payload.optString("windowId"))
                        append(" active=")
                        append(payload.optString("activeWindowId"))
                        append(" target=")
                        append(payload.optString("target"))
                        append(" corner=")
                        append(payload.optString("resizeCorner"))
                        append(" dx=")
                        append(payload.optString("dx"))
                        append(" dy=")
                        append(payload.optString("dy"))
                        append(" selectionTarget=")
                        append(payload.optString("selectionTarget"))
                        append(" selection=")
                        append(payload.optString("selectionText"))
                    },
                )
            }
            else -> {
                val id = payload.optString("windowId").toResultWindowIdOrNull() ?: return
                val active = resultWindows[id] ?: return
                when (payload.optString("type")) {
            "activateResultWindow" -> {
                setActiveResultWindow(id)
            }
            "dragResultWindow" -> {
                ensureDismissBubble()
                active.window.moveBy(
                    dx = payload.optDouble("dx", 0.0).roundToInt(),
                    dy = payload.optDouble("dy", 0.0).roundToInt(),
                    screenBounds = screenBounds(),
                )
                setActiveResultWindow(id)
            }
            "dragResultWindowAt" -> {
                updateDismissZone(
                    fingerBubbleProximity(
                        x = payload.optInt("x"),
                        y = payload.optInt("y"),
                    ),
                )
            }
            "dragResultWindowEnd" -> {
                val proximity = fingerBubbleProximity(
                    x = payload.optInt("x"),
                    y = payload.optInt("y"),
                )
                lastFingerDistSq = Int.MAX_VALUE
                hideDismissZone()
                if (proximity >= 0.8f) {
                    closeResultWindow(id)
                } else {
                    persistResultBounds(id, active.window.currentBounds())
                    ensureCanvasWindow()
                }
            }
            "resizeResultWindow" -> {
                resizeResultWindow(
                    active = active,
                    corner = payload.optString("corner"),
                    dx = payload.optDouble("dx", 0.0).roundToInt(),
                    dy = payload.optDouble("dy", 0.0).roundToInt(),
                )
                setActiveResultWindow(id)
                ensureCanvasWindow()
            }
            "resizeResultWindowEnd" -> {
                persistResultBounds(id, active.window.currentBounds())
                ensureCanvasWindow()
            }
            "cancelResultGesture" -> {
                lastFingerDistSq = Int.MAX_VALUE
                hideDismissZone()
            }
            "navigationState" -> {
                updateRuntimeState(id) { runtime ->
                    runtime.copy(
                        navDepth = payload.optInt("navDepth", runtime.navDepth),
                        maxNavDepth = payload.optInt("maxNavDepth", runtime.maxNavDepth),
                        isBrowsing = payload.optBoolean("isBrowsing", runtime.isBrowsing),
                    )
                }
                setActiveResultWindow(id)
            }
                }
            }
        }
    }

    private fun handleCanvasMessage(message: String) {
        val payload = message.jsonOrNull() ?: return
        when (payload.optString("action")) {
            "update_clickable_regions" -> {
                val regions = payload.optJSONArray("regions") ?: return
                canvasWindow?.updateTouchRegions(
                    List(regions.length()) { index ->
                        val region = regions.getJSONObject(index)
                        android.graphics.Rect(
                            region.optInt("x"),
                            region.optInt("y"),
                            region.optInt("x") + region.optInt("w"),
                            region.optInt("y") + region.optInt("h"),
                        )
                    },
                )
            }
            "copy" -> {
                val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
                clipboardManager.setPrimaryClip(
                    ClipData.newPlainText("preset_result", window.windowState.markdownText),
                )
                Toast.makeText(context, copyStatusText(), Toast.LENGTH_SHORT).show()
                updateRuntimeState(window.id) { it.copy(copySuccess = true) }
                setActiveResultWindow(window.id)
            }
            "back" -> {
                val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
                window.window.runScript("history.back();")
                updateRuntimeState(window.id) { runtime ->
                    val nextDepth = (runtime.navDepth - 1).coerceAtLeast(0)
                    runtime.copy(navDepth = nextDepth, isBrowsing = nextDepth > 0)
                }
                setActiveResultWindow(window.id)
            }
            "forward" -> {
                val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
                window.window.runScript("history.forward();")
                updateRuntimeState(window.id) { runtime ->
                    val nextDepth = (runtime.navDepth + 1).coerceAtMost(runtime.maxNavDepth)
                    runtime.copy(navDepth = nextDepth, isBrowsing = nextDepth > 0)
                }
                setActiveResultWindow(window.id)
            }
            "set_opacity" -> {
                val value = payload.optInt("value", 100)
                val id = payload.optString("hwnd").toResultWindowIdOrNull() ?: return
                updateRuntimeState(id) { it.copy(opacityPercent = value.coerceIn(10, 100)) }
                setActiveResultWindow(id)
            }
            "placeholder_action" -> {
                showPlaceholderAction(payload.optString("placeholder"))
            }
            "broom_drag_start" -> {
                val id = payload.optString("hwnd").toResultWindowIdOrNull() ?: return
                closeResultWindow(id)
            }
        }
    }

    private fun closeResultWindow(id: PresetResultWindowId) {
        resultWindows.remove(id)?.window?.destroy()
        if (activeResultWindowId == id) {
            activeResultWindowId = resultWindows.keys.firstOrNull()
        }
        ensureCanvasWindow()
        if (resultWindows.isEmpty() && inputWindow == null) {
            activePreset = null
        }
    }

    private fun submitInput(text: String) {
        val resolved = activePreset ?: return
        inputHistory.add(text)
        historyNavigationIndex = null
        historyDraftText = ""
        presetRepository.resetState()
        presetRepository.executePreset(resolved.preset, PresetInput.Text(text))
        if (resolved.preset.continuousInput) {
            inputWindow?.runScript("window.clearInput();")
        } else {
            requestCloseInputWindow(animate = true)
        }
    }

    private fun persistResultBounds(
        id: PresetResultWindowId,
        bounds: OverlayBounds,
    ) {
        if (resultWindows.keys.firstOrNull() != id) {
            ensureCanvasWindow()
            return
        }
        val presetId = activePreset?.preset?.id ?: return
        val geometry = WindowGeometry(
            x = bounds.x,
            y = bounds.y,
            width = bounds.width,
            height = bounds.height,
        )
        presetRepository.updateBuiltInOverride(presetId) { preset ->
            preset.copy(windowGeometry = geometry)
        }
    }

    private fun updateResultWindow(active: ActivePresetResultWindow) {
        val rendered = if (active.windowState.isError) {
            PresetRenderedContent(
                html = errorHtml(active.windowState.markdownText),
                isRawHtmlDocument = false,
            )
        } else {
            renderer.render(active.windowState.markdownText)
        }
        val updatedRuntime = active.runtimeState.copy(isRawHtml = rendered.isRawHtmlDocument)
        val updated = active.copy(runtimeState = updatedRuntime)
        resultWindows[active.id] = updated
        if (rendered.isRawHtmlDocument) {
            active.window.loadHtmlContent(rendered.html, RESULT_WINDOW_BASE_URL)
            active.window.runScript("window.configureResultWindow && window.configureResultWindow(${JSONObject.quote(active.id.wireValue())});")
        } else {
            if (active.runtimeState.isRawHtml) {
                active.window.loadHtmlContent(
                    resultHtmlBuilder.build(
                        PresetResultHtmlSettings(
                            isDark = isDarkTheme(uiPreferencesProvider().themeMode),
                        ),
                    ),
                    RESULT_WINDOW_BASE_URL,
                )
            }
            active.window.runScript(
                jsCall(
                    "applyResultState",
                    buildResultStatePayload(
                        windowId = active.id,
                        html = rendered.html,
                        windowState = active.windowState,
                    ),
                ),
            )
        }
        if (activeResultWindowId == null) {
            activeResultWindowId = active.id
        }
    }

    private fun updateRuntimeState(
        id: PresetResultWindowId,
        transform: (PresetResultWindowRuntimeState) -> PresetResultWindowRuntimeState,
    ) {
        val active = resultWindows[id] ?: return
        resultWindows[id] = active.copy(runtimeState = transform(active.runtimeState))
        ensureCanvasWindow()
    }

    private fun setActiveResultWindow(id: PresetResultWindowId) {
        if (!resultWindows.containsKey(id)) return
        activeResultWindowId = id
        ensureCanvasWindow()
    }

    private fun syncCanvasWindow(
        active: ActivePresetResultWindow,
        layout: PresetCanvasWindowLayout,
        lingerMs: Int,
    ) {
        val window = canvasWindow ?: return
        val bounds = layout.bounds
        if (window.currentBounds() != bounds) {
            window.updateBounds(bounds)
        }
        window.runScript(
            jsCall(
                "setCanvasWindows",
                buildCanvasPayload(
                    window = active,
                    vertical = layout.vertical,
                    lingerMs = lingerMs,
                ),
            ),
        )
    }

    private fun disabledActionsForWindow(): Set<String> {
        return setOf("undo", "redo", "edit", "download", "speaker", "markdown")
    }

    private fun resizeResultWindow(
        active: ActivePresetResultWindow,
        corner: String,
        dx: Int,
        dy: Int,
    ) {
        val current = active.window.currentBounds()
        val screen = screenBounds()
        val minWidth = dp(110)
        val minHeight = dp(90)
        val nextBounds = when (corner) {
            "br" -> current.copy(
                width = (current.width + dx).coerceIn(minWidth, screen.width() - current.x),
                height = (current.height + dy).coerceIn(minHeight, screen.height() - current.y),
            )
            "bl" -> {
                val nextWidth = (current.width - dx).coerceIn(minWidth, current.x + current.width)
                val nextX = (current.x + current.width - nextWidth).coerceAtLeast(0)
                current.copy(
                    x = nextX,
                    width = nextWidth,
                    height = (current.height + dy).coerceIn(minHeight, screen.height() - current.y),
                )
            }
            else -> current
        }
        active.window.updateBounds(nextBounds)
    }

    private fun showPlaceholderAction(action: String) {
        val message = when (action) {
            "edit", "undo", "redo" -> placeholderReasonLabel(PresetPlaceholderReason.GRAPH_EDITING_NOT_READY, uiLanguage())
            "download" -> placeholderReasonLabel(PresetPlaceholderReason.HTML_RESULT_NOT_READY, uiLanguage())
            "speaker" -> placeholderReasonLabel(PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY, uiLanguage())
            else -> localized("This action is not ready on Android yet.", "Tính năng này chưa sẵn sàng trên Android.", "이 기능은 아직 안드로이드에서 준비되지 않았습니다.")
        }
        Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
    }

    private fun favoritePresets(): List<ResolvedPreset> {
        return presetRepository.catalogState.value.presets.filter { it.preset.isFavorite }
    }

    private fun favoritePanelPresets(): List<ResolvedPreset> {
        return favoritePresets().filter { !it.preset.isUpcoming }
    }

    private fun buildPanelHtml(favorites: List<ResolvedPreset>): String {
        panelPresetIds = favorites.map { it.preset.id }
        return favoriteBubbleHtmlBuilder.build(
            FavoriteBubblePanelSettings(
                favorites = favorites,
                lang = uiLanguage(),
                isDark = isDarkTheme(uiPreferencesProvider().themeMode),
                keepOpenEnabled = keepOpenProvider(),
                columnCount = panelColumnCount(favorites.size),
            ),
        )
    }

    private fun buildInputHtml(resolvedPreset: ResolvedPreset): String {
        return textInputHtmlBuilder.build(
            PresetTextInputHtmlSettings(
                lang = uiLanguage(),
                title = resolvedPreset.preset.name(uiLanguage()),
                placeholder = localized("Type here...", "Nhập tại đây...", "여기에 입력하세요..."),
                isDark = isDarkTheme(uiPreferencesProvider().themeMode),
            ),
        )
    }

    private fun navigateInputHistory(
        current: String,
        upwards: Boolean,
    ) {
        if (inputHistory.isEmpty()) {
            return
        }
        val nextText = if (upwards) {
            navigateHistoryUp(current)
        } else {
            navigateHistoryDown(current)
        } ?: return
        inputWindow?.runScript("window.setEditorText(${JSONObject.quote(nextText)});")
    }

    private fun navigateHistoryUp(current: String): String? {
        if (inputHistory.isEmpty()) {
            return null
        }
        if (historyNavigationIndex == null) {
            historyDraftText = current
            historyNavigationIndex = inputHistory.lastIndex
            return inputHistory.lastOrNull()
        }
        val newIndex = (historyNavigationIndex!! - 1).coerceAtLeast(0)
        historyNavigationIndex = newIndex
        return inputHistory.getOrNull(newIndex)
    }

    private fun navigateHistoryDown(current: String): String? {
        val index = historyNavigationIndex ?: return null
        return if (index >= inputHistory.lastIndex) {
            historyNavigationIndex = null
            historyDraftText.ifEmpty { current }
        } else {
            val newIndex = index + 1
            historyNavigationIndex = newIndex
            inputHistory.getOrNull(newIndex)
        }
    }

    private fun ensureDismissBubble() {
        if (dismissBubbleView != null) return
        val bubbleSize = dp(56)
        val circle = View(context).apply {
            background = android.graphics.drawable.GradientDrawable().apply {
                shape = android.graphics.drawable.GradientDrawable.OVAL
                setColor(android.graphics.Color.argb(200, 60, 60, 60))
            }
            alpha = 0f
            scaleX = 0.4f
            scaleY = 0.4f
        }
        val icon = android.widget.TextView(context).apply {
            text = "\u00D7"
            textSize = 24f
            setTextColor(android.graphics.Color.WHITE)
            gravity = Gravity.CENTER
        }
        val container = FrameLayout(context).apply {
            addView(circle, FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                gravity = Gravity.CENTER
            })
            addView(icon, FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                gravity = Gravity.CENTER
            })
        }
        val params = WindowManager.LayoutParams(
            bubbleSize * 2,
            bubbleSize * 2,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            android.graphics.PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
            y = dp(24)
        }
        dismissBubbleView = container
        runCatching { windowManager.addView(container, params) }
        circle.animate()
            .alpha(1f).scaleX(1f).scaleY(1f)
            .setDuration(250)
            .setInterpolator(android.view.animation.OvershootInterpolator(1.5f))
            .start()
    }

    private fun updateDismissZone(proximity: Float) {
        ensureDismissBubble()
        val circle = (dismissBubbleView as? FrameLayout)?.getChildAt(0) ?: return
        val scale = 1f + proximity * 0.35f
        circle.scaleX = scale
        circle.scaleY = scale
        val r = (60 + (160 * proximity)).toInt().coerceIn(0, 255)
        val g = (60 - (10 * proximity)).toInt().coerceIn(0, 255)
        val b = (60 - (10 * proximity)).toInt().coerceIn(0, 255)
        val a = (200 + (20 * proximity)).toInt().coerceIn(0, 255)
        (circle.background as? android.graphics.drawable.GradientDrawable)
            ?.setColor(android.graphics.Color.argb(a, r, g, b))
    }

    private fun hideDismissZone() {
        val view = dismissBubbleView ?: return
        val circle = (view as? FrameLayout)?.getChildAt(0)
        if (circle != null) {
            circle.animate()
                .alpha(0f)
                .scaleX(0.3f)
                .scaleY(0.3f)
                .setDuration(200)
                .withEndAction {
                    runCatching { windowManager.removeView(view) }
                    dismissBubbleView = null
                }
                .start()
        } else {
            runCatching { windowManager.removeView(view) }
            dismissBubbleView = null
        }
    }

    private fun fingerBubbleProximity(rawXY: String): Float {
        val parts = rawXY.split(",")
        if (parts.size != 2) return 0f
        val fingerCssX = parts[0].toIntOrNull() ?: return 0f
        val fingerCssY = parts[1].toIntOrNull() ?: return 0f
        return fingerBubbleProximity(fingerCssX, fingerCssY)
    }

    private fun fingerBubbleProximity(
        x: Int,
        y: Int,
    ): Float {
        val screen = screenBounds()
        val bubbleCenterCssX = (screen.width() / density / 2).toInt()
        val bubbleCenterCssY = ((screen.height() - statusBarHeight() - dp(24) - dp(28)) / density).toInt()
        val dx = x - bubbleCenterCssX
        val dy = y - bubbleCenterCssY
        val distSq = dx * dx + dy * dy
        val approaching = distSq < lastFingerDistSq
        lastFingerDistSq = distSq
        val hitRadius = 55f
        val outerRadius = if (approaching) 140f else 110f
        val dist = kotlin.math.sqrt(distSq.toFloat())
        return if (dist <= hitRadius) {
            1f
        } else if (dist <= outerRadius) {
            1f - (dist - hitRadius) / (outerRadius - hitRadius)
        } else {
            0f
        }
    }

    private fun statusBarHeight(): Int {
        val resourceId = context.resources.getIdentifier("status_bar_height", "dimen", "android")
        return if (resourceId > 0) context.resources.getDimensionPixelSize(resourceId) else dp(24)
    }

    private fun syncPanelWindowState(window: PresetOverlayWindow) {
        val panelBounds = window.currentBounds()
        val bubbleCenterX = bubbleBounds.x + (bubbleBounds.width / 2)
        val bubbleCenterY = bubbleBounds.y + (bubbleBounds.height / 2)
        val bubbleCenterCssX = ((bubbleCenterX - panelBounds.x) / density).roundToInt()
        val bubbleCenterCssY = ((bubbleCenterY - panelBounds.y) / density).roundToInt()
        val bubbleOverlapCssPx = ((bubbleBounds.width / density) + 4f).roundToInt()
        val side = if (bubbleBounds.x > screenBounds().width() / 2) {
            FavoriteBubbleSide.RIGHT
        } else {
            FavoriteBubbleSide.LEFT
        }
        window.runScript(
            "window.updateBubbleCenter($bubbleCenterCssX,$bubbleCenterCssY);" +
                "window.setSide('${side.wireValue}', $bubbleOverlapCssPx);",
        )
    }

    private fun openPanelScript(window: PresetOverlayWindow): String {
        val panelBounds = window.currentBounds()
        val bubbleCenterX = bubbleBounds.x + (bubbleBounds.width / 2)
        val bubbleCenterY = bubbleBounds.y + (bubbleBounds.height / 2)
        val bubbleCenterCssX = ((bubbleCenterX - panelBounds.x) / density).roundToInt()
        val bubbleCenterCssY = ((bubbleCenterY - panelBounds.y) / density).roundToInt()
        return "window.animateIn($bubbleCenterCssX,$bubbleCenterCssY);"
    }

    private fun showPanelImmediatelyScript(): String {
        return "window.showItemsImmediately();"
    }

    private fun panelWindowSpec(itemCount: Int): PresetOverlayWindowSpec {
        val screen = screenBounds()
        val screenCssWidth = screen.width() / density
        val screenCssHeight = screen.height() / density
        val overlapCssWidth = (bubbleBounds.width / density) + PANEL_OVERLAP_MARGIN_CSS
        val overlapWidth = cssToPhysical(overlapCssWidth)
        val desiredColumns = desiredPanelColumnCount(itemCount)
        val columns = panelColumnCount(itemCount)
        val columnWidthCss = panelColumnWidthCss(columns)
        val itemsPerColumn = if (itemCount > 0) {
            itemCount.divCeil(columns)
        } else {
            0
        }

        val panelBodyWidthCss = if (itemCount == 0) {
            EMPTY_PANEL_WIDTH_CSS.coerceAtMost((screenCssWidth - overlapCssWidth - PANEL_EDGE_GUTTER_CSS).coerceAtLeast(PANEL_COLUMN_WIDTH_CSS.toFloat()))
        } else {
            ((columnWidthCss * columns) + PANEL_WIDTH_BUFFER_CSS)
                .coerceAtMost((screenCssWidth - overlapCssWidth - PANEL_EDGE_GUTTER_CSS).coerceAtLeast(columnWidthCss))
        }
        val panelBodyWidth = cssToPhysical(panelBodyWidthCss)
        val width = panelBodyWidth + overlapWidth
        val contentHeightCss = if (itemCount == 0) {
            EMPTY_PANEL_HEIGHT_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS
        } else {
            (itemsPerColumn * PANEL_ITEM_HEIGHT_CSS) +
                (PANEL_TOP_PADDING_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS + PANEL_EXTRA_BOTTOM_CSS)
        }
        val maxHeightCss = screenCssHeight * PANEL_MAX_HEIGHT_SCREEN_RATIO
        val height = cssToPhysical(contentHeightCss.toFloat().coerceAtMost(maxHeightCss))
        val x = if (bubbleBounds.x > screenBounds().width() / 2) {
            (bubbleBounds.x - panelBodyWidth - dp(4)).coerceAtLeast(0)
        } else {
            bubbleBounds.x.coerceAtMost((screenBounds().width() - width).coerceAtLeast(0))
        }
        val y = (bubbleBounds.y - (height / 2) + (bubbleBounds.height / 2))
            .coerceIn(0, (screenBounds().height() - height).coerceAtLeast(0))
        return PresetOverlayWindowSpec(
            width = width,
            height = height,
            x = x,
            y = y,
            focusable = false,
        )
    }

    private fun minPanelHeight(itemCount: Int): Int {
        val columns = panelColumnCount(itemCount)
        val itemsPerColumn = if (itemCount > 0) itemCount.divCeil(columns) else 0
        val heightCss = if (itemCount == 0) {
            EMPTY_PANEL_HEIGHT_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS
        } else {
            (itemsPerColumn * PANEL_ITEM_HEIGHT_CSS) +
                (PANEL_TOP_PADDING_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS + PANEL_EXTRA_BOTTOM_CSS)
        }
        return cssToPhysical(heightCss)
    }

    private fun panelColumnCount(itemCount: Int): Int {
        val screen = screenBounds()
        val screenCssWidth = screen.width() / density
        val screenCssHeight = screen.height() / density
        val overlapCssWidth = (bubbleBounds.width / density) + PANEL_OVERLAP_MARGIN_CSS
        val desiredColumns = desiredPanelColumnCount(itemCount)
        val maxItemsPerColumn = maxItemsPerColumn(screenCssHeight).coerceAtLeast(1)
        val heightDrivenColumns = if (itemCount > 0) itemCount.divCeil(maxItemsPerColumn) else 1
        val requestedColumns = maxOf(desiredColumns, heightDrivenColumns)
        val columnWidthCss = panelColumnWidthCss(requestedColumns)
        val availableColumns = maxColumnsForScreen(screenCssWidth, overlapCssWidth, columnWidthCss)
        return requestedColumns.coerceAtMost(availableColumns)
    }

    private fun desiredPanelColumnCount(itemCount: Int): Int {
        return if (itemCount > WINDOWS_ITEMS_PER_COLUMN) {
            itemCount.divCeil(WINDOWS_ITEMS_PER_COLUMN)
        } else {
            1
        }
    }

    private fun maxItemsPerColumn(screenCssHeight: Float): Int {
        val maxHeightCss = screenCssHeight * PANEL_MAX_HEIGHT_SCREEN_RATIO
        val contentBudget = maxHeightCss - (
            PANEL_TOP_PADDING_CSS +
                PANEL_HEIGHT_BUFFER_CSS +
                KEEP_OPEN_ROW_HEIGHT_CSS +
                PANEL_EXTRA_BOTTOM_CSS
            )
        return (contentBudget / PANEL_ITEM_HEIGHT_CSS).toInt().coerceAtLeast(1)
    }

    private fun panelColumnWidthCss(columnCount: Int): Float {
        return if (columnCount > 1) MOBILE_MULTI_COLUMN_WIDTH_CSS else PANEL_COLUMN_WIDTH_CSS.toFloat()
    }

    private fun maxColumnsForScreen(screenCssWidth: Float, overlapCssWidth: Float, columnWidthCss: Float): Int {
        val usableWidth = (screenCssWidth - overlapCssWidth - PANEL_EDGE_GUTTER_CSS).coerceAtLeast(columnWidthCss)
        val withBuffer = (usableWidth - PANEL_WIDTH_BUFFER_CSS).coerceAtLeast(columnWidthCss)
        return (withBuffer / columnWidthCss).toInt().coerceAtLeast(1)
    }

    private fun inputWindowSpec(htmlContent: String): PresetOverlayWindowSpec {
        val width = dp(340)
        val height = dp(228)
        val screen = screenBounds()
        return PresetOverlayWindowSpec(
            width = width,
            height = height,
            x = ((screen.width() - width) / 2).coerceAtLeast(0),
            y = (screen.height() * 0.14f).roundToInt(),
            focusable = true,
            htmlContent = htmlContent,
            baseUrl = INPUT_WINDOW_BASE_URL,
        )
    }

    private fun resultWindowSpec(
        resolvedPreset: ResolvedPreset,
        windowState: PresetResultWindowState,
        placed: List<PresetResultWindowPlacement>,
    ): PresetOverlayWindowSpec {
        val saved = resolvedPreset.preset.windowGeometry
        val width = saved?.width?.takeIf { it > 0 } ?: dp(340)
        val height = saved?.height?.takeIf { it > 0 } ?: dp(280)
        val bounds = if (windowState.overlayOrder == 0) {
            val screen = screenBounds()
            OverlayBounds(
                width = width,
                height = height,
                x = saved?.x?.coerceIn(0, (screen.width() - width).coerceAtLeast(0))
                    ?: ((screen.width() - width) / 2).coerceAtLeast(0),
                y = saved?.y?.coerceIn(0, (screen.height() - height).coerceAtLeast(0))
                    ?: (screen.height() * 0.28f).roundToInt(),
            )
        } else {
            nextResultBounds(
                previous = placed.lastOrNull()?.bounds ?: OverlayBounds(
                    x = dp(24),
                    y = dp(140),
                    width = width,
                    height = height,
                ),
                width = width,
                height = height,
            )
        }
        return PresetOverlayWindowSpec(
            width = bounds.width,
            height = bounds.height,
            x = bounds.x,
            y = bounds.y,
            focusable = true,
            htmlContent = resultHtmlBuilder.build(
                PresetResultHtmlSettings(
                    isDark = isDarkTheme(uiPreferencesProvider().themeMode),
                ),
            ),
            baseUrl = RESULT_WINDOW_BASE_URL,
        )
    }

    private fun canvasWindowSpec(bounds: OverlayBounds): PresetOverlayWindowSpec {
        return PresetOverlayWindowSpec(
            width = bounds.width,
            height = bounds.height,
            x = bounds.x,
            y = bounds.y,
            focusable = false,
            clipToOutline = false,
            touchRegionsOnly = true,
        )
    }

    private fun canvasWindowLayout(active: ActivePresetResultWindow): PresetCanvasWindowLayout {
        val resultBounds = active.window.currentBounds()
        val screen = screenBounds()
        val gap = dp(CANVAS_MARGIN_DP)
        val buttonCount = visibleCanvasButtonCount(active)
        val horizontalWidth = cssToPhysical(horizontalCanvasWidthCss(buttonCount)).coerceAtMost(screen.width())
        val horizontalHeight = cssToPhysical(CANVAS_HORIZONTAL_HEIGHT_CSS).coerceAtMost(screen.height())
        val verticalWidth = cssToPhysical(CANVAS_VERTICAL_WIDTH_CSS).coerceAtMost(screen.width())
        val verticalHeight = cssToPhysical(verticalCanvasHeightCss(buttonCount)).coerceAtMost(screen.height())

        val spaceBottom = screen.height() - (resultBounds.y + resultBounds.height)
        val spaceTop = resultBounds.y
        val spaceRight = screen.width() - (resultBounds.x + resultBounds.width)
        val spaceLeft = resultBounds.x

        val layout = when {
            spaceBottom >= horizontalHeight + gap -> {
                val x = (resultBounds.x + resultBounds.width - horizontalWidth)
                    .coerceIn(0, (screen.width() - horizontalWidth).coerceAtLeast(0))
                val y = (resultBounds.y + resultBounds.height + gap)
                    .coerceIn(0, (screen.height() - horizontalHeight).coerceAtLeast(0))
                PresetCanvasWindowLayout(
                    bounds = OverlayBounds(x = x, y = y, width = horizontalWidth, height = horizontalHeight),
                    vertical = false,
                )
            }
            spaceRight >= verticalWidth + gap -> {
                val x = (resultBounds.x + resultBounds.width + gap)
                    .coerceIn(0, (screen.width() - verticalWidth).coerceAtLeast(0))
                val y = (resultBounds.y + (resultBounds.height - verticalHeight) / 2)
                    .coerceIn(0, (screen.height() - verticalHeight).coerceAtLeast(0))
                PresetCanvasWindowLayout(
                    bounds = OverlayBounds(x = x, y = y, width = verticalWidth, height = verticalHeight),
                    vertical = true,
                )
            }
            spaceLeft >= verticalWidth + gap -> {
                val x = (resultBounds.x - verticalWidth - gap)
                    .coerceIn(0, (screen.width() - verticalWidth).coerceAtLeast(0))
                val y = (resultBounds.y + (resultBounds.height - verticalHeight) / 2)
                    .coerceIn(0, (screen.height() - verticalHeight).coerceAtLeast(0))
                PresetCanvasWindowLayout(
                    bounds = OverlayBounds(x = x, y = y, width = verticalWidth, height = verticalHeight),
                    vertical = true,
                )
            }
            spaceTop >= horizontalHeight + gap -> {
                val x = (resultBounds.x + (resultBounds.width - horizontalWidth) / 2)
                    .coerceIn(0, (screen.width() - horizontalWidth).coerceAtLeast(0))
                val y = (resultBounds.y - horizontalHeight - gap)
                    .coerceIn(0, (screen.height() - horizontalHeight).coerceAtLeast(0))
                PresetCanvasWindowLayout(
                    bounds = OverlayBounds(x = x, y = y, width = horizontalWidth, height = horizontalHeight),
                    vertical = false,
                )
            }
            else -> {
                val width = horizontalWidth.coerceAtMost((resultBounds.width - gap * 2).coerceAtLeast(cssToPhysical(220)))
                val x = (resultBounds.x + gap).coerceIn(0, (screen.width() - width).coerceAtLeast(0))
                val y = (resultBounds.y + resultBounds.height - horizontalHeight - gap)
                    .coerceIn(resultBounds.y, (screen.height() - horizontalHeight).coerceAtLeast(resultBounds.y))
                PresetCanvasWindowLayout(
                    bounds = OverlayBounds(x = x, y = y, width = width, height = horizontalHeight),
                    vertical = false,
                )
            }
        }
        return layout
    }

    private fun visibleCanvasButtonCount(active: ActivePresetResultWindow): Int {
        return 7
    }

    private fun horizontalCanvasWidthCss(buttonCount: Int): Int {
        val safeCount = buttonCount.coerceAtLeast(1)
        return (safeCount * CANVAS_BUTTON_SIZE_CSS) +
            ((safeCount - 1) * CANVAS_BUTTON_GAP_CSS) +
            CANVAS_HORIZONTAL_PADDING_CSS
    }

    private fun verticalCanvasHeightCss(buttonCount: Int): Int {
        val safeCount = buttonCount.coerceAtLeast(1)
        return (safeCount * (CANVAS_BUTTON_SIZE_CSS + CANVAS_VERTICAL_BUTTON_MARGIN_CSS)) +
            CANVAS_VERTICAL_PADDING_CSS
    }

    private fun nextResultBounds(
        previous: OverlayBounds,
        width: Int,
        height: Int,
    ): OverlayBounds {
        val gap = dp(14)
        val screen = screenBounds()
        val candidates = listOf(
            OverlayBounds(previous.x + previous.width + gap, previous.y, width, height),
            OverlayBounds(previous.x, previous.y + previous.height + gap, width, height),
            OverlayBounds(previous.x - width - gap, previous.y, width, height),
            OverlayBounds(previous.x, previous.y - height - gap, width, height),
        )
        return candidates.firstOrNull { candidate ->
            candidate.x >= 0 &&
                candidate.y >= 0 &&
                candidate.x + candidate.width <= screen.width() &&
                candidate.y + candidate.height <= screen.height() &&
                resultWindows.values.none { active ->
                    val bounds = active.window.currentBounds()
                    !(candidate.x + candidate.width <= bounds.x ||
                        bounds.x + bounds.width <= candidate.x ||
                        candidate.y + candidate.height <= bounds.y ||
                        bounds.y + bounds.height <= candidate.y)
                }
        } ?: OverlayBounds(
            x = (previous.x + dp(36)).coerceIn(0, (screen.width() - width).coerceAtLeast(0)),
            y = (previous.y + dp(36)).coerceIn(0, (screen.height() - height).coerceAtLeast(0)),
            width = width,
            height = height,
        )
    }

    private fun screenBounds(): Rect {
        val metrics = context.resources.displayMetrics
        return Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
    }

    private fun uiLanguage(): String = uiPreferencesProvider().uiLanguage

    private fun jsCall(
        functionName: String,
        jsonPayload: String,
    ): String {
        return "window.$functionName(${JSONObject.quote(jsonPayload)});"
    }

    private fun loadingHtml(): String {
        return "<p>${localized("Waiting for result...", "Đang đợi kết quả...", "결과를 기다리는 중...")}</p>"
    }

    private fun errorHtml(error: String): String {
        return "<p>${escapeHtml(error)}</p>"
    }

    private fun loadingStatusText(): String = localized("Loading", "Đang tải", "로딩")

    private fun streamingStatusText(): String = localized("Streaming", "Đang truyền", "스트리밍")

    private fun readyStatusText(): String = localized("Ready", "Sẵn sàng", "준비됨")

    private fun errorStatusText(): String = localized("Error", "Lỗi", "오류")

    private fun copyStatusText(): String = localized("Copied", "Đã sao chép", "복사됨")

    private fun localized(
        en: String,
        vi: String,
        ko: String,
    ): String = when (uiLanguage()) {
        "vi" -> vi
        "ko" -> ko
        else -> en
    }

    private fun isDarkTheme(themeMode: MobileThemeMode): Boolean {
        return when (themeMode) {
            MobileThemeMode.DARK -> true
            MobileThemeMode.LIGHT -> false
            MobileThemeMode.SYSTEM -> {
                val nightModeFlags = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
                nightModeFlags == Configuration.UI_MODE_NIGHT_YES
            }
        }
    }

    private fun dp(value: Int): Int = (value * density).roundToInt()

    private fun cssToPhysical(value: Float): Int = (value * density).roundToInt()

    private fun cssToPhysical(value: Int): Int = cssToPhysical(value.toFloat())

    private companion object {
        private const val TAG = "PresetOverlay"
        private const val FAVORITE_PANEL_BASE_URL = "file:///android_asset/realtime_overlay/"
        private const val INPUT_WINDOW_BASE_URL = "file:///android_asset/realtime_overlay/"
        private const val RESULT_WINDOW_BASE_URL = "file:///android_asset/preset_overlay/"
        private const val CANVAS_WINDOW_BASE_URL = "file:///android_asset/preset_overlay/"
        private const val CANVAS_LINGER_MS = 2000
        private const val CANVAS_MARGIN_DP = 4
        private const val CANVAS_HORIZONTAL_HEIGHT_CSS = 34
        private const val CANVAS_VERTICAL_WIDTH_CSS = 34
        private const val CANVAS_BUTTON_SIZE_CSS = 24
        private const val CANVAS_BUTTON_GAP_CSS = 4
        private const val CANVAS_HORIZONTAL_PADDING_CSS = 8
        private const val CANVAS_VERTICAL_BUTTON_MARGIN_CSS = 6
        private const val CANVAS_VERTICAL_PADDING_CSS = 12
        private const val WINDOWS_ITEMS_PER_COLUMN = 15
        private const val PANEL_COLUMN_WIDTH_CSS = 200
        private const val PANEL_WIDTH_BUFFER_CSS = 40f
        private const val MOBILE_MULTI_COLUMN_WIDTH_CSS = 140f
        private const val PANEL_HEIGHT_BUFFER_CSS = 60
        private const val KEEP_OPEN_ROW_HEIGHT_CSS = 40
        private const val PANEL_ITEM_HEIGHT_CSS = 48
        private const val PANEL_EXTRA_BOTTOM_CSS = 100
        private const val PANEL_TOP_PADDING_CSS = 24
        private const val EMPTY_PANEL_WIDTH_CSS = 320f
        private const val EMPTY_PANEL_HEIGHT_CSS = 80
        private const val PANEL_OVERLAP_MARGIN_CSS = 4f
        private const val PANEL_EDGE_GUTTER_CSS = 10f
        private const val PANEL_MAX_HEIGHT_SCREEN_RATIO = 0.62f
    }
}

private fun String.jsonOrNull(): JSONObject? = runCatching { JSONObject(this) }.getOrNull()

private fun Int.divCeil(divisor: Int): Int = (this + divisor - 1) / divisor

private data class PresetCanvasWindowLayout(
    val bounds: OverlayBounds,
    val vertical: Boolean,
)

private fun String.toResultWindowIdOrNull(): PresetResultWindowId? {
    val sessionId = substringBeforeLast(':', "")
    val blockIdx = substringAfterLast(':', "").toIntOrNull() ?: return null
    if (sessionId.isEmpty()) return null
    return PresetResultWindowId(sessionId = sessionId, blockIdx = blockIdx)
}

private fun escapeHtml(value: String): String {
    return value
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}
