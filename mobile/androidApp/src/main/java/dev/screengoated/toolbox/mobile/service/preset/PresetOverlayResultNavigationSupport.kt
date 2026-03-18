package dev.screengoated.toolbox.mobile.service.preset

import android.util.Log
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import org.json.JSONObject

internal fun PresetOverlayResultModule.handleResultPageFinishedSupport(
    id: PresetResultWindowId,
    url: String?,
) {
    val active = resultWindows[id] ?: return
    val currentUrl = url ?: active.window.currentUrl()
    Log.d(
        PresetOverlayResultModule.TAG,
        "handleResultPageFinished id=${id.wireValue()} url=$currentUrl raw=${active.runtimeState.isRawHtml} browsing=${active.runtimeState.isBrowsing}",
    )
    if (active.runtimeState.isBrowsing && isInternalResultUrl(currentUrl)) {
        restoreOriginalResultSurfaceSupport(id)
        return
    }
    val shouldInjectHostedShell =
        active.runtimeState.isRawHtml ||
            active.runtimeState.isBrowsing ||
            isExternalNavigationUrl(currentUrl)
    if (!shouldInjectHostedShell) return
    active.window.runScript(
        presetHostedRawPageBootstrapScript(
            windowId = id.wireValue(),
            isDark = isDarkTheme(),
        ),
    )
    syncResultNavigationStateSupport(id, currentUrl)
}

internal fun PresetOverlayResultModule.handleResultNavigationFailureSupport(
    id: PresetResultWindowId,
    failure: OverlayNavigationFailure,
) {
    val active = resultWindows[id] ?: return
    Log.w(
        PresetOverlayResultModule.TAG,
        "handleResultNavigationFailure id=${id.wireValue()} url=${failure.url} desc=${failure.description} browsing=${active.runtimeState.isBrowsing}",
    )
    if (!active.runtimeState.isBrowsing) return
    active.window.stopLoading()
    restoreOriginalResultSurfaceSupport(id)
    Toast.makeText(
        context,
        overlayLocalized(
            uiLanguage(),
            "That page could not be opened in the overlay.",
            "Trang đó không thể mở trong overlay.",
            "해당 페이지를 오버레이에서 열 수 없습니다.",
        ),
        Toast.LENGTH_SHORT,
    ).show()
}

internal fun PresetOverlayResultModule.syncResultNavigationStateSupport(id: PresetResultWindowId, url: String?) {
    val active = resultWindows[id] ?: return
    val history = active.window.historyState()
    val currentUrl = url ?: history.currentUrl
    val sync = computeResultNavigationSyncSupport(
        runtimeState = active.runtimeState,
        historyState = history,
        url = currentUrl,
    )
    if (sync.shouldRestoreOriginalSurface) {
        Log.d(
            PresetOverlayResultModule.TAG,
            "syncResultNavigationState restoring base external surface id=${id.wireValue()} url=$currentUrl currentIndex=${history.currentIndex} baseIndex=${sync.runtimeState.historyBaseIndex}",
        )
        restoreOriginalResultSurfaceSupport(id)
        return
    }
    Log.d(
        PresetOverlayResultModule.TAG,
        "syncResultNavigationState id=${id.wireValue()} url=$currentUrl currentIndex=${history.currentIndex} lastIndex=${history.lastIndex} baseIndex=${sync.runtimeState.historyBaseIndex} depth=${sync.runtimeState.navDepth} browsing=${sync.runtimeState.isBrowsing}",
    )
    updateRuntimeState(id) {
        sync.runtimeState.copy(
            opacityPercent = it.opacityPercent,
            copySuccess = it.copySuccess,
            disabledActions = it.disabledActions,
            isRawHtml = it.isRawHtml,
        )
    }
}

internal fun PresetOverlayResultModule.restoreOriginalResultSurfaceSupport(id: PresetResultWindowId) {
    val active = resultWindows[id] ?: return
    Log.d(PresetOverlayResultModule.TAG, "restoreOriginalResultSurface id=${id.wireValue()} raw=${active.runtimeState.isRawHtml} browsing=${active.runtimeState.isBrowsing}")
    active.window.stopLoading()
    val replacementWindow = recreateResultOverlayWindowSupport(
        active = active,
        context = context,
        windowManager = windowManager,
        onMessage = ::handleResultMessage,
        onPageFinished = ::handleResultPageFinished,
        onNavigationFailure = ::handleResultNavigationFailure,
    )
    val rendered = if (active.windowState.isError) {
        PresetRenderedContent(
            html = errorHtml(active.windowState.markdownText),
            isRawHtmlDocument = false,
        )
    } else {
        renderer.render(active.windowState.markdownText, isDarkTheme())
    }
    val updated = active.copy(
        runtimeState = active.runtimeState.copy(
            navDepth = 0,
            maxNavDepth = 0,
            historyBaseIndex = 0,
            isBrowsing = false,
            isRawHtml = rendered.isRawHtmlDocument,
        ),
        window = replacementWindow,
    )
    resultWindows[id] = updated
    if (rendered.isRawHtmlDocument) {
        replacementWindow.loadHtmlContent(rendered.html, RESULT_WINDOW_BASE_URL, clearHistoryAfterLoad = true)
        replacementWindow.runScript("window.configureResultWindow && window.configureResultWindow(${JSONObject.quote(id.wireValue())});")
    } else {
        replacementWindow.loadHtmlContent(
            resultHtmlBuilder.build(PresetResultHtmlSettings(isDark = isDarkTheme())),
            RESULT_WINDOW_BASE_URL,
            clearHistoryAfterLoad = true,
        )
        replacementWindow.runScript(
            "window.applyResultState(${JSONObject.quote(buildResultStatePayload(id, rendered.html, active.windowState))});",
        )
    }
    ensureCanvasWindowSupport()
}
