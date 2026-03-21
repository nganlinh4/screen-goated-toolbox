package dev.screengoated.toolbox.mobile.service.preset

import android.graphics.Rect
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import kotlin.math.roundToInt

internal const val FAVORITE_PANEL_BASE_URL: String = "file:///android_asset/realtime_overlay/"
internal const val INPUT_WINDOW_BASE_URL: String = "file:///android_asset/realtime_overlay/"
internal const val RESULT_WINDOW_BASE_URL: String = "file:///android_asset/preset_overlay/"
internal const val CANVAS_WINDOW_BASE_URL: String = "file:///android_asset/preset_overlay/"
internal const val CANVAS_LINGER_MS: Int = 2000
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
private const val PANEL_HEIGHT_BUFFER_CSS = 24
private const val KEEP_OPEN_ROW_HEIGHT_CSS = 40
private const val PANEL_ITEM_HEIGHT_CSS = 48
private const val PANEL_EXTRA_BOTTOM_CSS = 16
private const val PANEL_TOP_PADDING_CSS = 24
private const val EMPTY_PANEL_WIDTH_CSS = 320f
private const val EMPTY_PANEL_HEIGHT_CSS = 80
private const val PANEL_OVERLAP_MARGIN_CSS = 4f
private const val PANEL_EDGE_GUTTER_CSS = 10f
private const val PANEL_MAX_HEIGHT_SCREEN_RATIO = 0.62f

internal data class PresetPanelHtmlBuild(
    val html: String,
    val presetIds: List<String>,
)

internal fun buildPanelHtmlSupport(
    builder: FavoriteBubbleHtmlBuilder,
    favorites: List<ResolvedPreset>,
    uiLanguage: String,
    isDark: Boolean,
    keepOpenEnabled: Boolean,
    columnCount: Int,
): PresetPanelHtmlBuild {
    return PresetPanelHtmlBuild(
        html = builder.build(
            FavoriteBubblePanelSettings(
                favorites = favorites,
                lang = uiLanguage,
                isDark = isDark,
                keepOpenEnabled = keepOpenEnabled,
                columnCount = columnCount,
            ),
        ),
        presetIds = favorites.map { it.preset.id },
    )
}

internal fun buildInputHtmlSupport(
    builder: PresetTextInputHtmlBuilder,
    resolvedPreset: ResolvedPreset,
    uiLanguage: String,
    isDark: Boolean,
    placeholder: String,
): String {
    return builder.build(
        PresetTextInputHtmlSettings(
            lang = uiLanguage,
            title = resolvedPreset.preset.name(uiLanguage),
            placeholder = placeholder,
            isDark = isDark,
        ),
    )
}

internal fun navigateHistoryUpSupport(
    inputHistory: List<String>,
    historyNavigationIndex: Int?,
    current: String,
): Pair<Int?, String?> {
    if (inputHistory.isEmpty()) {
        return historyNavigationIndex to null
    }
    if (historyNavigationIndex == null) {
        return inputHistory.lastIndex to inputHistory.lastOrNull()
    }
    val newIndex = (historyNavigationIndex - 1).coerceAtLeast(0)
    return newIndex to inputHistory.getOrNull(newIndex)
}

internal fun navigateHistoryDownSupport(
    inputHistory: List<String>,
    historyNavigationIndex: Int?,
    current: String,
    historyDraftText: String,
): Pair<Int?, String?> {
    val index = historyNavigationIndex ?: return null to null
    return if (index >= inputHistory.lastIndex) {
        null to historyDraftText.ifEmpty { current }
    } else {
        val newIndex = index + 1
        newIndex to inputHistory.getOrNull(newIndex)
    }
}

internal fun syncPanelWindowStateScriptSupport(
    panelBounds: OverlayBounds,
    bubbleBounds: OverlayBounds,
    density: Float,
    screenBounds: Rect,
): String {
    val bubbleCenterX = bubbleBounds.x + (bubbleBounds.width / 2)
    val bubbleCenterY = bubbleBounds.y + (bubbleBounds.height / 2)
    val bubbleCenterCssX = ((bubbleCenterX - panelBounds.x) / density).roundToInt()
    val bubbleCenterCssY = ((bubbleCenterY - panelBounds.y) / density).roundToInt()
    val bubbleOverlapCssPx = panelBubbleOverlapCssWidthSupport(bubbleBounds, density).roundToInt()
    val side = if (bubbleBounds.x > screenBounds.width() / 2) {
        FavoriteBubbleSide.RIGHT
    } else {
        FavoriteBubbleSide.LEFT
    }
    return "window.updateBubbleCenter($bubbleCenterCssX,$bubbleCenterCssY);" +
        "window.setSide('${side.wireValue}', $bubbleOverlapCssPx);"
}

internal fun openPanelScriptSupport(
    panelBounds: OverlayBounds,
    bubbleBounds: OverlayBounds,
    density: Float,
): String {
    val bubbleCenterX = bubbleBounds.x + (bubbleBounds.width / 2)
    val bubbleCenterY = bubbleBounds.y + (bubbleBounds.height / 2)
    val bubbleCenterCssX = ((bubbleCenterX - panelBounds.x) / density).roundToInt()
    val bubbleCenterCssY = ((bubbleCenterY - panelBounds.y) / density).roundToInt()
    return "window.animateIn($bubbleCenterCssX,$bubbleCenterCssY);"
}

internal fun showPanelImmediatelyScriptSupport(): String = "window.showItemsImmediately();"

internal fun panelWindowSpecSupport(
    itemCount: Int,
    bubbleBounds: OverlayBounds,
    density: Float,
    screenBounds: Rect,
    cssToPhysical: (Float) -> Int,
): PresetOverlayWindowSpec {
    val screenCssWidth = screenBounds.width() / density
    val screenCssHeight = screenBounds.height() / density
    val overlapCssWidth = panelBubbleOverlapCssWidthSupport(bubbleBounds, density)
    val overlapWidth = cssToPhysical(overlapCssWidth)
    val columns = panelColumnCountSupport(itemCount, bubbleBounds, density, screenBounds)
    val columnWidthCss = panelColumnWidthCss(columns)
    val itemsPerColumn = if (itemCount > 0) itemCount.divCeil(columns) else 0

    val panelBodyWidthCss = if (itemCount == 0) {
        EMPTY_PANEL_WIDTH_CSS.coerceAtMost(
            (screenCssWidth - overlapCssWidth - PANEL_EDGE_GUTTER_CSS).coerceAtLeast(PANEL_COLUMN_WIDTH_CSS.toFloat()),
        )
    } else {
        ((columnWidthCss * columns) + PANEL_WIDTH_BUFFER_CSS)
            .coerceAtMost((screenCssWidth - overlapCssWidth - PANEL_EDGE_GUTTER_CSS).coerceAtLeast(columnWidthCss))
    }
    val panelBodyWidth = cssToPhysical(panelBodyWidthCss)
    // Match Windows: panel content stays beside the bubble, but the window extends
    // behind it so bloom/collapse animations originate from the bubble itself.
    val width = panelBodyWidth + overlapWidth
    val contentHeightCss = if (itemCount == 0) {
        EMPTY_PANEL_HEIGHT_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS
    } else {
        (itemsPerColumn * PANEL_ITEM_HEIGHT_CSS) +
            (PANEL_TOP_PADDING_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS + PANEL_EXTRA_BOTTOM_CSS)
    }
    val maxHeightCss = screenCssHeight * PANEL_MAX_HEIGHT_SCREEN_RATIO
    val height = cssToPhysical(contentHeightCss.toFloat().coerceAtMost(maxHeightCss))
    val gap = cssToPhysical(PANEL_OVERLAP_MARGIN_CSS)
    val x = if (bubbleBounds.x > screenBounds.width() / 2) {
        (bubbleBounds.x - panelBodyWidth - gap).coerceAtLeast(0)
    } else {
        bubbleBounds.x.coerceAtMost((screenBounds.width() - width).coerceAtLeast(0))
    }
    val y = (bubbleBounds.y - (height / 2) + (bubbleBounds.height / 2))
        .coerceIn(0, (screenBounds.height() - height).coerceAtLeast(0))
    return PresetOverlayWindowSpec(
        width = width,
        height = height,
        x = x,
        y = y,
        focusable = false,
    )
}

internal fun minPanelHeightSupport(
    itemCount: Int,
    bubbleBounds: OverlayBounds,
    density: Float,
    screenBounds: Rect,
    cssToPhysical: (Float) -> Int,
): Int {
    val columns = panelColumnCountSupport(itemCount, bubbleBounds, density, screenBounds)
    val itemsPerColumn = if (itemCount > 0) itemCount.divCeil(columns) else 0
    val heightCss = if (itemCount == 0) {
        EMPTY_PANEL_HEIGHT_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS
    } else {
        (itemsPerColumn * PANEL_ITEM_HEIGHT_CSS) +
            (PANEL_TOP_PADDING_CSS + PANEL_HEIGHT_BUFFER_CSS + KEEP_OPEN_ROW_HEIGHT_CSS + PANEL_EXTRA_BOTTOM_CSS)
    }
    return cssToPhysical(heightCss.toFloat())
}

internal fun panelColumnCountSupport(
    itemCount: Int,
    bubbleBounds: OverlayBounds,
    density: Float,
    screenBounds: Rect,
): Int {
    val screenCssWidth = screenBounds.width() / density
    val screenCssHeight = screenBounds.height() / density
    val overlapCssWidth = panelBubbleOverlapCssWidthSupport(bubbleBounds, density)
    val desiredColumns = desiredPanelColumnCount(itemCount)
    val maxItemsPerColumn = maxItemsPerColumn(screenCssHeight).coerceAtLeast(1)
    val heightDrivenColumns = if (itemCount > 0) itemCount.divCeil(maxItemsPerColumn) else 1
    val requestedColumns = maxOf(desiredColumns, heightDrivenColumns)
    val columnWidthCss = panelColumnWidthCss(requestedColumns)
    val availableColumns = maxColumnsForScreen(screenCssWidth, overlapCssWidth, columnWidthCss)
    return requestedColumns.coerceAtMost(availableColumns)
}

private fun desiredPanelColumnCount(itemCount: Int): Int {
    return if (itemCount > WINDOWS_ITEMS_PER_COLUMN) itemCount.divCeil(WINDOWS_ITEMS_PER_COLUMN) else 1
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

internal fun panelBubbleOverlapCssWidthSupport(
    bubbleBounds: OverlayBounds,
    density: Float,
): Float {
    return (bubbleBounds.width / density) + PANEL_OVERLAP_MARGIN_CSS
}

private fun maxColumnsForScreen(screenCssWidth: Float, overlapCssWidth: Float, columnWidthCss: Float): Int {
    val usableWidth = (screenCssWidth - overlapCssWidth - PANEL_EDGE_GUTTER_CSS).coerceAtLeast(columnWidthCss)
    val withBuffer = (usableWidth - PANEL_WIDTH_BUFFER_CSS).coerceAtLeast(columnWidthCss)
    return (withBuffer / columnWidthCss).toInt().coerceAtLeast(1)
}

internal fun inputWindowSpecSupport(
    htmlContent: String,
    screenBounds: Rect,
    dp: (Int) -> Int,
): PresetOverlayWindowSpec {
    val width = dp(340)
    val height = dp(228)
    return PresetOverlayWindowSpec(
        width = width,
        height = height,
        x = ((screenBounds.width() - width) / 2).coerceAtLeast(0),
        y = (screenBounds.height() * 0.14f).roundToInt(),
        focusable = true,
        showImeOnFocus = true,
        htmlContent = htmlContent,
        baseUrl = INPUT_WINDOW_BASE_URL,
    )
}

internal fun resultWindowSpecSupport(
    resolvedPreset: ResolvedPreset,
    windowState: PresetResultWindowState,
    placed: List<PresetResultWindowPlacement>,
    screenBounds: Rect,
    dp: (Int) -> Int,
    buildHtml: () -> String,
): PresetOverlayWindowSpec {
    val saved = resolvedPreset.preset.windowGeometry
    val width = saved?.width?.takeIf { it > 0 } ?: dp(340)
    val height = saved?.height?.takeIf { it > 0 } ?: dp(280)
    val bounds = if (windowState.overlayOrder == 0) {
        OverlayBounds(
            width = width,
            height = height,
            x = saved?.x?.coerceIn(0, (screenBounds.width() - width).coerceAtLeast(0))
                ?: ((screenBounds.width() - width) / 2).coerceAtLeast(0),
            y = saved?.y?.coerceIn(0, (screenBounds.height() - height).coerceAtLeast(0))
                ?: (screenBounds.height() * 0.28f).roundToInt(),
        )
    } else {
        nextResultBoundsSupport(
            previous = placed.lastOrNull()?.bounds ?: OverlayBounds(
                x = dp(24),
                y = dp(140),
                width = width,
                height = height,
            ),
            width = width,
            height = height,
            screenBounds = screenBounds,
            occupiedBounds = placed.map { it.bounds },
            dp = dp,
        )
    }
    return PresetOverlayWindowSpec(
        width = bounds.width,
        height = bounds.height,
        x = bounds.x,
        y = bounds.y,
        focusable = true,
        htmlContent = buildHtml(),
        baseUrl = RESULT_WINDOW_BASE_URL,
    )
}

internal fun canvasWindowSpecSupport(bounds: OverlayBounds): PresetOverlayWindowSpec {
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

internal fun canvasWindowLayoutSupport(
    resultBounds: OverlayBounds,
    screenBounds: Rect,
    buttonCount: Int,
    dp: (Int) -> Int,
    cssToPhysical: (Int) -> Int,
): PresetCanvasWindowLayout {
    val gap = dp(CANVAS_MARGIN_DP)
    val horizontalWidth = cssToPhysical(horizontalCanvasWidthCss(buttonCount)).coerceAtMost(screenBounds.width())
    val horizontalHeight = cssToPhysical(CANVAS_HORIZONTAL_HEIGHT_CSS).coerceAtMost(screenBounds.height())
    val verticalWidth = cssToPhysical(CANVAS_VERTICAL_WIDTH_CSS).coerceAtMost(screenBounds.width())
    val verticalHeight = cssToPhysical(verticalCanvasHeightCss(buttonCount)).coerceAtMost(screenBounds.height())

    val spaceBottom = screenBounds.height() - (resultBounds.y + resultBounds.height)
    val spaceTop = resultBounds.y
    val spaceRight = screenBounds.width() - (resultBounds.x + resultBounds.width)
    val spaceLeft = resultBounds.x

    return when {
        spaceBottom >= horizontalHeight + gap -> {
            val x = (resultBounds.x + resultBounds.width - horizontalWidth)
                .coerceIn(0, (screenBounds.width() - horizontalWidth).coerceAtLeast(0))
            val y = (resultBounds.y + resultBounds.height + gap)
                .coerceIn(0, (screenBounds.height() - horizontalHeight).coerceAtLeast(0))
            PresetCanvasWindowLayout(
                bounds = OverlayBounds(x = x, y = y, width = horizontalWidth, height = horizontalHeight),
                vertical = false,
            )
        }
        spaceRight >= verticalWidth + gap -> {
            val x = (resultBounds.x + resultBounds.width + gap)
                .coerceIn(0, (screenBounds.width() - verticalWidth).coerceAtLeast(0))
            val y = (resultBounds.y + (resultBounds.height - verticalHeight) / 2)
                .coerceIn(0, (screenBounds.height() - verticalHeight).coerceAtLeast(0))
            PresetCanvasWindowLayout(
                bounds = OverlayBounds(x = x, y = y, width = verticalWidth, height = verticalHeight),
                vertical = true,
            )
        }
        spaceLeft >= verticalWidth + gap -> {
            val x = (resultBounds.x - verticalWidth - gap)
                .coerceIn(0, (screenBounds.width() - verticalWidth).coerceAtLeast(0))
            val y = (resultBounds.y + (resultBounds.height - verticalHeight) / 2)
                .coerceIn(0, (screenBounds.height() - verticalHeight).coerceAtLeast(0))
            PresetCanvasWindowLayout(
                bounds = OverlayBounds(x = x, y = y, width = verticalWidth, height = verticalHeight),
                vertical = true,
            )
        }
        spaceTop >= horizontalHeight + gap -> {
            val x = (resultBounds.x + (resultBounds.width - horizontalWidth) / 2)
                .coerceIn(0, (screenBounds.width() - horizontalWidth).coerceAtLeast(0))
            val y = (resultBounds.y - horizontalHeight - gap)
                .coerceIn(0, (screenBounds.height() - horizontalHeight).coerceAtLeast(0))
            PresetCanvasWindowLayout(
                bounds = OverlayBounds(x = x, y = y, width = horizontalWidth, height = horizontalHeight),
                vertical = false,
            )
        }
        else -> {
            val width = horizontalWidth.coerceAtMost((resultBounds.width - gap * 2).coerceAtLeast(cssToPhysical(220)))
            val x = (resultBounds.x + gap).coerceIn(0, (screenBounds.width() - width).coerceAtLeast(0))
            val y = (resultBounds.y + resultBounds.height - horizontalHeight - gap)
                .coerceIn(resultBounds.y, (screenBounds.height() - horizontalHeight).coerceAtLeast(resultBounds.y))
            PresetCanvasWindowLayout(
                bounds = OverlayBounds(x = x, y = y, width = width, height = horizontalHeight),
                vertical = false,
            )
        }
    }
}

internal fun visibleCanvasButtonCountSupport(): Int = 7

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

internal fun nextResultBoundsSupport(
    previous: OverlayBounds,
    width: Int,
    height: Int,
    screenBounds: Rect,
    occupiedBounds: List<OverlayBounds>,
    dp: (Int) -> Int,
): OverlayBounds {
    val gap = dp(14)
    val candidates = listOf(
        OverlayBounds(previous.x + previous.width + gap, previous.y, width, height),
        OverlayBounds(previous.x, previous.y + previous.height + gap, width, height),
        OverlayBounds(previous.x - width - gap, previous.y, width, height),
        OverlayBounds(previous.x, previous.y - height - gap, width, height),
    )
    return candidates.firstOrNull { candidate ->
        candidate.x >= 0 &&
            candidate.y >= 0 &&
            candidate.x + candidate.width <= screenBounds.width() &&
            candidate.y + candidate.height <= screenBounds.height() &&
            occupiedBounds.none { bounds ->
                !(candidate.x + candidate.width <= bounds.x ||
                    bounds.x + bounds.width <= candidate.x ||
                    candidate.y + candidate.height <= bounds.y ||
                    bounds.y + bounds.height <= candidate.y)
            }
    } ?: OverlayBounds(
        x = (previous.x + dp(36)).coerceIn(0, (screenBounds.width() - width).coerceAtLeast(0)),
        y = (previous.y + dp(36)).coerceIn(0, (screenBounds.height() - height).coerceAtLeast(0)),
        width = width,
        height = height,
    )
}
