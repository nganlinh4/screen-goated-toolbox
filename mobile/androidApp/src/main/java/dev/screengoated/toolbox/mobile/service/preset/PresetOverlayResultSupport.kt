package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.service.resizeOverlayFromCorner

internal data class PresetResultNavigationSync(
    val runtimeState: PresetResultWindowRuntimeState,
    val shouldRestoreOriginalSurface: Boolean,
)

internal fun createResultOverlayWindowSupport(
    context: Context,
    windowManager: WindowManager,
    spec: PresetOverlayWindowSpec,
    id: PresetResultWindowId,
    onMessage: (String) -> Unit,
    onPageFinished: (PresetResultWindowId, String?) -> Unit,
    onNavigationFailure: (PresetResultWindowId, OverlayNavigationFailure) -> Unit,
    isDark: Boolean,
    uiLanguage: String,
): PresetOverlayWindow {
    return PresetOverlayWindow(
        context = context,
        windowManager = windowManager,
        spec = spec.copy(
            resultWindowId = id.wireValue(), resultIsDark = isDark,
            resultCapabilityMessage = overlayLocalized(
                uiLanguage,
                "Some overlay controls are unavailable. Reading and navigation still work.",
                "Một số điều khiển overlay không khả dụng. Bạn vẫn có thể đọc và điều hướng.",
                "일부 오버레이 제어를 사용할 수 없습니다. 읽기와 탐색은 계속 사용할 수 있습니다.",
            ),
        ),
        onMessage = onMessage,
    ).also { window ->
        window.setOnPageFinishedListener { url -> onPageFinished(id, url) }
        window.setOnNavigationFailureListener { failure ->
            onNavigationFailure(id, failure)
        }
        window.show()
    }
}

internal fun recreateResultOverlayWindowSupport(
    active: ActivePresetResultWindow,
    context: Context,
    windowManager: WindowManager,
    onMessage: (String) -> Unit,
    onPageFinished: (PresetResultWindowId, String?) -> Unit,
    onNavigationFailure: (PresetResultWindowId, OverlayNavigationFailure) -> Unit,
    isDark: Boolean,
    uiLanguage: String,
): PresetOverlayWindow {
    val bounds = active.window.currentBounds()
    active.window.setOnPageFinishedListener(null)
    active.window.setOnNavigationFailureListener(null)
    active.window.destroy()
    return createResultOverlayWindowSupport(
        context = context,
        windowManager = windowManager,
        spec = PresetOverlayWindowSpec(
            width = bounds.width,
            height = bounds.height,
            x = bounds.x,
            y = bounds.y,
            focusable = true,
            baseUrl = RESULT_WINDOW_BASE_URL,
        ),
        id = active.id,
        onMessage = onMessage,
        onPageFinished = onPageFinished,
        onNavigationFailure = onNavigationFailure,
        isDark = isDark,
        uiLanguage = uiLanguage,
    )
}

internal fun syncCanvasWindowSupport(
    window: PresetOverlayWindow,
    active: ActivePresetResultWindow,
    layout: PresetCanvasWindowLayout,
    lingerMs: Int,
) {
    if (window.currentBounds() != layout.bounds) {
        window.updateBounds(layout.bounds)
    }
    window.runScript(
        "window.setCanvasWindows(${org.json.JSONObject.quote(buildCanvasPayload(window = active, vertical = layout.vertical, lingerMs = lingerMs))});",
    )
}

internal fun resizeResultWindowSupport(
    active: ActivePresetResultWindow,
    corner: String,
    dx: Int,
    dy: Int,
    screenBounds: Rect,
    dp: (Int) -> Int,
) {
    val current = active.window.currentBounds()
    val nextBounds = resizeOverlayFromCorner(
        current = current,
        corner = corner,
        dx = dx,
        dy = dy,
        screenWidth = screenBounds.width(),
        screenHeight = screenBounds.height(),
        minWidth = dp(110),
        minHeight = dp(90),
    ) ?: current
    active.window.updateBounds(nextBounds)
}

internal fun computeResultNavigationSyncSupport(
    runtimeState: PresetResultWindowRuntimeState,
    historyState: OverlayHistoryState,
    url: String?,
): PresetResultNavigationSync {
    val currentUrl = url ?: historyState.currentUrl
    if (isInternalResultUrl(currentUrl)) {
        return PresetResultNavigationSync(
            runtimeState = runtimeState.copy(
                navDepth = 0,
                maxNavDepth = 0,
                historyBaseIndex = historyState.currentIndex,
                isBrowsing = false,
            ),
            shouldRestoreOriginalSurface = false,
        )
    }
    val external = isExternalNavigationUrl(currentUrl)
    val baseIndex = if (!runtimeState.isBrowsing && !runtimeState.isRawHtml && !external) {
        historyState.currentIndex
    } else {
        runtimeState.historyBaseIndex
    }
    val relativeDepth = (historyState.currentIndex - baseIndex).coerceAtLeast(0)
    val shouldRestoreBaseExternalSurface =
        external &&
            historyState.currentIndex <= baseIndex &&
            (runtimeState.isBrowsing || runtimeState.maxNavDepth > 0)
    return PresetResultNavigationSync(
        runtimeState = runtimeState.copy(
            navDepth = relativeDepth,
            maxNavDepth = maxOf(runtimeState.maxNavDepth, relativeDepth),
            historyBaseIndex = baseIndex,
            isBrowsing = relativeDepth > 0 || (external && historyState.currentIndex > baseIndex),
        ),
        shouldRestoreOriginalSurface = shouldRestoreBaseExternalSurface,
    )
}

internal fun showPlaceholderActionSupport(
    context: Context,
    action: String,
    uiLanguage: String,
) {
    val message = when (action) {
        "edit", "undo", "redo" -> placeholderReasonLabel(PresetPlaceholderReason.GRAPH_EDITING_NOT_READY, uiLanguage)
        "download" -> placeholderReasonLabel(PresetPlaceholderReason.HTML_RESULT_NOT_READY, uiLanguage)
        "speaker" -> placeholderReasonLabel(PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY, uiLanguage)
        else -> overlayLocalized(uiLanguage, "This action is not ready on Android yet.", "Tính năng này chưa sẵn sàng trên Android.", "이 기능은 아직 안드로이드에서 준비되지 않았습니다.")
    }
    Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
}
