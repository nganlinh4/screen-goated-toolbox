package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.util.Log
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.service.OverlayBounds

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
): PresetOverlayWindow {
    return PresetOverlayWindow(
        context = context,
        windowManager = windowManager,
        spec = spec,
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

internal fun disabledActionsForWindowSupport(): Set<String> {
    return setOf("undo", "redo", "edit", "download", "speaker", "markdown")
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
    val minWidth = dp(110)
    val minHeight = dp(90)
    val nextBounds = when (corner) {
        "br" -> current.copy(
            width = (current.width + dx).coerceIn(minWidth, screenBounds.width() - current.x),
            height = (current.height + dy).coerceIn(minHeight, screenBounds.height() - current.y),
        )
        "bl" -> {
            val nextWidth = (current.width - dx).coerceIn(minWidth, current.x + current.width)
            val nextX = (current.x + current.width - nextWidth).coerceAtLeast(0)
            current.copy(
                x = nextX,
                width = nextWidth,
                height = (current.height + dy).coerceIn(minHeight, screenBounds.height() - current.y),
            )
        }
        else -> current
    }
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

internal fun logResultGestureDebug(tag: String, payload: org.json.JSONObject) {
    Log.d(
        tag,
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
            append(" keep=")
            append(payload.optString("keepTextSelection"))
            append(" point=(")
            append(payload.optString("clientX"))
            append(",")
            append(payload.optString("clientY"))
            append(")")
            append(" delay=")
            append(payload.optString("delayMs"))
            append(" error=")
            append(payload.optString("error"))
            append(" scroll=(")
            append(payload.optString("scrollX"))
            append(",")
            append(payload.optString("scrollY"))
            append(")")
            append(" start=(")
            append(payload.optString("startLeft"))
            append(",")
            append(payload.optString("startTop"))
            append("..")
            append(payload.optString("startBottom"))
            append(")")
            append(" end=(")
            append(payload.optString("endLeft"))
            append(",")
            append(payload.optString("endTop"))
            append("..")
            append(payload.optString("endBottom"))
            append(")")
            if (payload.has("candidatePoint")) append(" candidate=").append(payload.opt("candidatePoint"))
            if (payload.has("fixedPoint")) append(" fixed=").append(payload.opt("fixedPoint"))
            if (payload.has("nextAnchor")) append(" nextAnchor=").append(payload.opt("nextAnchor"))
            if (payload.has("nextFocus")) append(" nextFocus=").append(payload.opt("nextFocus"))
            if (payload.has("anchorPoint")) append(" anchor=").append(payload.opt("anchorPoint"))
            if (payload.has("focusPoint")) append(" focus=").append(payload.opt("focusPoint"))
            if (payload.has("rangeStart")) append(" rangeStart=").append(payload.opt("rangeStart"))
            if (payload.has("rangeEnd")) append(" rangeEnd=").append(payload.opt("rangeEnd"))
        },
    )
}
