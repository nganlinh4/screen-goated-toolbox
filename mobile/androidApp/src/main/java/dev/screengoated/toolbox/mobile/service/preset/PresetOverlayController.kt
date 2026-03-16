package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.res.Configuration
import android.graphics.Rect
import android.util.Log
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import org.json.JSONObject
import kotlin.math.roundToInt

internal class PresetOverlayController(
    private val context: Context,
    private val scope: CoroutineScope,
    private val windowManager: WindowManager,
    private val presetRepository: PresetRepository,
    private val uiPreferencesProvider: () -> MobileUiPreferences,
    private val keepOpenProvider: () -> Boolean,
    private val onKeepOpenChanged: (Boolean) -> Unit,
    private val onIncreaseBubbleSize: () -> Unit,
    private val onDecreaseBubbleSize: () -> Unit,
    private val onPanelExpandedChanged: (Boolean) -> Unit = {},
    private val onRequestBubbleFront: () -> Unit = {},
) {
    private val clipboardManager = context.getSystemService(ClipboardManager::class.java)
    private val renderer = PresetMarkdownRenderer()
    private val favoriteBubbleHtmlBuilder = FavoriteBubbleHtmlBuilder()
    private val density = context.resources.displayMetrics.density

    private var panelWindow: PresetOverlayWindow? = null
    private var inputWindow: PresetOverlayWindow? = null
    private var resultWindow: PresetOverlayWindow? = null
    private var canvasWindow: PresetOverlayWindow? = null

    private var activePreset: ResolvedPreset? = null
    private var currentResultText: String = ""
    private var bubbleBounds = OverlayBounds(x = 0, y = 0, width = dp(48), height = dp(48))
    private var panelPresetIds: List<String> = emptyList()
    private var panelClosing = false

    private var catalogJob: Job? = null
    private var executionJob: Job? = null

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
        onPanelExpandedChanged(false)
        panelWindow?.destroy()
        inputWindow?.destroy()
        resultWindow?.destroy()
        canvasWindow?.destroy()
        panelWindow = null
        inputWindow = null
        resultWindow = null
        canvasWindow = null
        activePreset = null
        currentResultText = ""
        presetRepository.resetState()
    }

    private fun openPanel() {
        val favorites = favoritePanelPresets()
        if (favorites.isEmpty()) {
            Toast.makeText(context, emptyFavoritesMessage(uiLanguage()), Toast.LENGTH_SHORT).show()
            return
        }
        panelClosing = false
        Log.d(TAG, "Opening panel favorites=${favorites.size}")
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
        Log.d(TAG, "Reloading panel html favorites=${favorites.size}")
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
        closeResultWindow(resetExecution = false)
        presetRepository.resetState()
        activePreset = resolved
        currentResultText = ""
        openInputWindow(resolved)
        if (!closePanel) {
            onRequestBubbleFront()
        }
    }

    private fun openInputWindow(resolvedPreset: ResolvedPreset) {
        val spec = inputWindowSpec()
        if (inputWindow == null) {
            inputWindow = PresetOverlayWindow(
                context = context,
                windowManager = windowManager,
                spec = spec,
                onMessage = ::handleInputMessage,
            )
        }
        inputWindow?.updateBounds(
            OverlayBounds(
                x = spec.x,
                y = spec.y,
                width = spec.width,
                height = spec.height,
            ),
        )
        inputWindow?.show()
        inputWindow?.runScript(
            jsCall(
                "applyInputBootstrap",
                buildInputBootstrap(resolvedPreset.preset, uiLanguage()),
            ),
        )
    }

    private fun closeInputWindow() {
        inputWindow?.destroy()
        inputWindow = null
    }

    private fun ensureResultWindow() {
        val resolvedPreset = activePreset ?: return
        if (resultWindow == null) {
            val spec = resultWindowSpec(resolvedPreset)
            resultWindow = PresetOverlayWindow(
                context = context,
                windowManager = windowManager,
                spec = spec,
                onMessage = ::handleResultMessage,
                onBoundsChanged = ::persistResultBounds,
            )
        }
        resultWindow?.show()
        resultWindow?.runScript(
            jsCall(
                "applyResultBootstrap",
                buildResultBootstrap(
                    preset = resolvedPreset.preset,
                    lang = uiLanguage(),
                    status = loadingStatusText(),
                ),
            ),
        )
        ensureCanvasWindow()
    }

    private fun ensureCanvasWindow() {
        val result = resultWindow ?: return
        val resultBounds = result.currentBounds()
        val spec = canvasWindowSpec(resultBounds)
        if (canvasWindow == null) {
            canvasWindow = PresetOverlayWindow(
                context = context,
                windowManager = windowManager,
                spec = spec,
                onMessage = ::handleCanvasMessage,
            )
            canvasWindow?.show()
            canvasWindow?.runScript(jsCall("applyCanvasBootstrap", buildCanvasBootstrap(uiLanguage())))
            return
        }
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

    private fun closeResultWindow(resetExecution: Boolean = true) {
        presetRepository.cancelExecution()
        resultWindow?.destroy()
        resultWindow = null
        canvasWindow?.destroy()
        canvasWindow = null
        currentResultText = ""
        if (resetExecution) {
            presetRepository.resetState()
        }
    }

    private fun renderExecutionState(state: PresetExecutionState) {
        val activeId = activePreset?.preset?.id ?: return
        if (state.activePreset?.id != null && state.activePreset.id != activeId) {
            return
        }
        if (state.activePreset == null && currentResultText.isEmpty()) {
            return
        }

        ensureResultWindow()
        val result = resultWindow ?: return
        val preset = activePreset?.preset ?: return

        when {
            state.error != null -> {
                currentResultText = state.error
                result.runScript(
                    jsCall(
                        "updateResultState",
                        buildResultUpdatePayload(
                            preset = preset,
                            html = errorHtml(state.error),
                            status = errorStatusText(),
                            streaming = false,
                            lang = uiLanguage(),
                        ),
                    ),
                )
            }
            state.isExecuting && state.streamingText.isBlank() && currentResultText.isBlank() -> {
                result.runScript(
                    jsCall(
                        "updateResultState",
                        buildResultUpdatePayload(
                            preset = preset,
                            html = loadingHtml(),
                            status = loadingStatusText(),
                            streaming = true,
                            lang = uiLanguage(),
                        ),
                    ),
                )
            }
            state.streamingText.isNotBlank() -> {
                currentResultText = state.streamingText
                result.runScript(
                    jsCall(
                        "updateResultState",
                        buildResultUpdatePayload(
                            preset = preset,
                            html = renderer.render(state.streamingText),
                            status = if (state.isExecuting) {
                                streamingStatusText()
                            } else {
                                readyStatusText()
                            },
                            streaming = state.isExecuting,
                            lang = uiLanguage(),
                        ),
                    ),
                )
            }
        }

        ensureCanvasWindow()
    }

    private fun handlePanelMessage(message: String) {
        when {
            message == "dismiss" || message == "close_now" -> closePanel(animate = false)
            message == "focus_bubble" -> onRequestBubbleFront()
            message == "panel_ready" -> {
                Log.d(TAG, "Panel WebView ready")
                onRequestBubbleFront()
            }
            message.startsWith("resize:") -> {
                val window = panelWindow ?: return
                val measuredHeight = message.substringAfter("resize:", "").toIntOrNull() ?: return
                val clampedHeight = measuredHeight
                    .coerceAtLeast(minPanelHeight(panelPresetIds.size))
                    .coerceAtMost((screenBounds().height() * 0.62f).roundToInt())
                Log.d(TAG, "Panel measured height=$measuredHeight clamped=$clampedHeight")
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
                    "panelRendered" -> Log.d(TAG, "Panel rendered items=${payload.optInt("itemCount", -1)}")
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
        val payload = message.jsonOrNull() ?: return
        when (payload.optString("type")) {
            "dragInputWindow" -> {
                inputWindow?.moveBy(
                    dx = payload.optDouble("dx", 0.0).roundToInt(),
                    dy = payload.optDouble("dy", 0.0).roundToInt(),
                    screenBounds = screenBounds(),
                )
            }
            "closeInputWindow" -> {
                closeInputWindow()
                if (currentResultText.isEmpty()) {
                    activePreset = null
                }
            }
            "submitInput" -> {
                val text = payload.optString("text").trim()
                if (text.isNotEmpty()) {
                    submitInput(text)
                }
            }
        }
    }

    private fun handleResultMessage(message: String) {
        val payload = message.jsonOrNull() ?: return
        if (payload.optString("type") == "dragResultWindow") {
            resultWindow?.moveBy(
                dx = payload.optDouble("dx", 0.0).roundToInt(),
                dy = payload.optDouble("dy", 0.0).roundToInt(),
                screenBounds = screenBounds(),
            )
            resultWindow?.let { ensureCanvasWindow() }
        }
    }

    private fun handleCanvasMessage(message: String) {
        val payload = message.jsonOrNull() ?: return
        when (payload.optString("type")) {
            "copyResult" -> {
                if (currentResultText.isNotBlank()) {
                    clipboardManager.setPrimaryClip(
                        ClipData.newPlainText("preset_result", currentResultText),
                    )
                    Toast.makeText(context, copyStatusText(), Toast.LENGTH_SHORT).show()
                }
            }
            "closeResult" -> {
                closeResultWindow()
                if (inputWindow == null) {
                    activePreset = null
                }
            }
        }
    }

    private fun submitInput(text: String) {
        val resolved = activePreset ?: return
        ensureResultWindow()
        presetRepository.resetState()
        presetRepository.executePreset(resolved.preset, PresetInput.Text(text))
        if (resolved.preset.continuousInput) {
            inputWindow?.runScript("window.clearInput();")
        } else {
            closeInputWindow()
        }
    }

    private fun persistResultBounds(bounds: OverlayBounds) {
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
        ensureCanvasWindow()
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

    private fun inputWindowSpec(): PresetOverlayWindowSpec {
        val width = dp(340)
        val height = dp(228)
        val screen = screenBounds()
        return PresetOverlayWindowSpec(
            width = width,
            height = height,
            x = ((screen.width() - width) / 2).coerceAtLeast(0),
            y = (screen.height() * 0.14f).roundToInt(),
            focusable = true,
            assetPage = "input.html",
        )
    }

    private fun resultWindowSpec(resolvedPreset: ResolvedPreset): PresetOverlayWindowSpec {
        val saved = resolvedPreset.preset.windowGeometry
        val width = saved?.width?.takeIf { it > 0 } ?: dp(340)
        val height = saved?.height?.takeIf { it > 0 } ?: dp(280)
        val screen = screenBounds()
        val x = saved?.x?.coerceIn(0, (screen.width() - width).coerceAtLeast(0))
            ?: ((screen.width() - width) / 2).coerceAtLeast(0)
        val y = saved?.y?.coerceIn(0, (screen.height() - height).coerceAtLeast(0))
            ?: (screen.height() * 0.28f).roundToInt()
        return PresetOverlayWindowSpec(
            width = width,
            height = height,
            x = x,
            y = y,
            focusable = false,
            assetPage = "result.html",
        )
    }

    private fun canvasWindowSpec(resultBounds: OverlayBounds): PresetOverlayWindowSpec {
        val width = dp(144)
        val height = dp(54)
        val screen = screenBounds()
        val x = (resultBounds.x + resultBounds.width - width - dp(10))
            .coerceIn(0, (screen.width() - width).coerceAtLeast(0))
        val y = (resultBounds.y + dp(10)).coerceIn(0, (screen.height() - height).coerceAtLeast(0))
        return PresetOverlayWindowSpec(
            width = width,
            height = height,
            x = x,
            y = y,
            focusable = false,
            assetPage = "button_canvas.html",
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

private fun escapeHtml(value: String): String {
    return value
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}
