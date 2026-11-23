package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry
import org.json.JSONObject

internal fun PresetOverlayResultModule.refreshResultWindowsForThemeSupport() {
    resultWindows.values.forEach { active ->
        if (active.runtimeState.isBrowsing || active.runtimeState.isRawHtml) return@forEach
        active.window.loadHtmlContent(
            resultHtmlBuilder.build(PresetResultHtmlSettings(isDark = isDarkTheme())),
            RESULT_WINDOW_BASE_URL,
        )
        updateResultWindowSupport(active)
    }
}

internal fun PresetOverlayResultModule.syncResultWindowsSupport(
    windowStates: List<PresetResultWindowState>,
    activePreset: ResolvedPreset,
) {
    val sessionId = windowStates.firstOrNull()?.id?.sessionId ?: return
    val targetIds = windowStates.map { it.id }.toSet()
    resultWindows.keys
        .filter { it.sessionId == sessionId }
        .filterNot(targetIds::contains)
        .toList()
        .forEach { id ->
            refineJobs.cancel(id)
            resultWindows.remove(id)?.window?.destroy()
            if (activeResultWindowId == id) activeResultWindowId = null
            if (topmostResultWindowId == id) topmostResultWindowId = null
        }

    val placed = mutableListOf<PresetResultWindowPlacement>()
    windowStates.sortedBy { it.overlayOrder }.forEach { windowState ->
        val existing = resultWindows[windowState.id]
        val defaultOpacity = overlayOpacityProvider()
        val runtime = (existing?.runtimeState ?: PresetResultWindowRuntimeState(
            opacityPercent = defaultOpacity,
        )).copy(disabledActions = resultDisabledActions(windowState))
        val window = existing?.window ?: createResultOverlayWindowSupport(
            context = context,
            windowManager = windowManager,
            spec = resultWindowSpecSupport(
                resolvedPreset = activePreset,
                windowState = windowState,
                placed = placed,
                screenBounds = screenBoundsProvider(),
                dp = dp,
                buildHtml = {
                    resultHtmlBuilder.build(PresetResultHtmlSettings(isDark = isDarkTheme()))
                },
            ),
            id = windowState.id,
            onMessage = ::handleResultMessage,
            onPageFinished = ::handleResultPageFinished,
            onNavigationFailure = ::handleResultNavigationFailure,
            isDark = isDarkTheme(),
            uiLanguage = uiLanguage(),
        )
        if (existing == null && runtime.opacityPercent < 100) {
            window.setWindowAlpha(runtime.opacityPercent / 100f)
        }
        val bounds = window.currentBounds()
        placed += PresetResultWindowPlacement(windowState.id, bounds)
        // Only re-render if the window state actually changed or the window is new.
        // Prevents sibling overlays from having their loading animation reset or
        // triggering unnecessary DOM re-layout when another overlay streams chunks.
        // (Windows achieves this via per-HWND state in HashMap — each window updates independently.)
        val stateChanged = existing == null || existing.windowState != windowState
        val active = ActivePresetResultWindow(
            id = windowState.id,
            presetId = activePreset.preset.id,
            runtimeState = runtime,
            windowState = windowState,
            window = window,
        )
        if (stateChanged) {
            updateResultWindowSupport(active)
        } else {
            resultWindows[active.id] = active
        }
        if (topmostResultWindowId == null) {
            topmostResultWindowId = windowState.id
        }
    }
}

internal fun PresetOverlayResultModule.syncStandaloneResultWindowsSupport(
    windowStates: List<PresetResultWindowState>,
) {
    val sessionId = windowStates.firstOrNull()?.id?.sessionId ?: return
    val standalonePresetId = "$STANDALONE_RESULT_PRESET_PREFIX$sessionId"
    val targetIds = windowStates.map { it.id }.toSet()
    resultWindows.keys
        .filter { it.sessionId == sessionId }
        .filterNot(targetIds::contains)
        .toList()
        .forEach { id ->
            refineJobs.cancel(id)
            resultWindows.remove(id)?.window?.destroy()
            if (activeResultWindowId == id) activeResultWindowId = null
            if (topmostResultWindowId == id) topmostResultWindowId = null
        }

    val placed = mutableListOf<PresetResultWindowPlacement>()
    windowStates.sortedBy { it.overlayOrder }.forEach { windowState ->
        val existing = resultWindows[windowState.id]
        val defaultOpacity = overlayOpacityProvider()
        val runtime = (existing?.runtimeState ?: PresetResultWindowRuntimeState(
            opacityPercent = defaultOpacity,
        )).copy(disabledActions = resultWindowDisabledActions(standalonePresetId, windowState))
        val window = existing?.window ?: createResultOverlayWindowSupport(
            context = context,
            windowManager = windowManager,
            spec = standaloneResultWindowSpecSupport(
                windowState = windowState,
                placed = placed,
                screenBounds = screenBoundsProvider(),
                dp = dp,
                buildHtml = {
                    resultHtmlBuilder.build(PresetResultHtmlSettings(isDark = isDarkTheme()))
                },
            ),
            id = windowState.id,
            onMessage = ::handleResultMessage,
            onPageFinished = ::handleResultPageFinished,
            onNavigationFailure = ::handleResultNavigationFailure,
            isDark = isDarkTheme(),
            uiLanguage = uiLanguage(),
        )
        if (existing == null && runtime.opacityPercent < 100) {
            window.setWindowAlpha(runtime.opacityPercent / 100f)
        }
        val bounds = window.currentBounds()
        placed += PresetResultWindowPlacement(windowState.id, bounds)
        val stateChanged = existing == null || existing.windowState != windowState
        val active = ActivePresetResultWindow(
            id = windowState.id,
            presetId = standalonePresetId,
            runtimeState = runtime,
            windowState = windowState,
            window = window,
        )
        if (stateChanged) {
            updateResultWindowSupport(active)
        } else {
            resultWindows[active.id] = active
        }
        if (topmostResultWindowId == null) {
            topmostResultWindowId = windowState.id
        }
    }
}

/**
 * [notifyWhenEmpty] is false when the running graph retires a window itself:
 * the preset stays active until the graph finishes, as it does for a preset
 * whose blocks show no window at all.
 */
internal fun PresetOverlayResultModule.closeResultWindowSupport(
    id: PresetResultWindowId,
    notifyWhenEmpty: Boolean = true,
) {
    refineJobs.cancel(id)
    val removed = resultWindows.remove(id)
    if (removed != null) {
        if (removed.runtimeState.ttsRequestId != 0L) {
            ttsRuntimeService?.stopIfActive(removed.runtimeState.ttsRequestId)
        }
        removed.window.destroy()
    }
    if (activeResultWindowId == id) {
        activeResultWindowId = resultWindows.keys.firstOrNull()
    }
    if (topmostResultWindowId == id) {
        topmostResultWindowId = activeResultWindowId
    }
    canvasSuspendedForGesture = false
    ensureCanvasWindowSupport()
    if (notifyWhenEmpty && resultWindows.isEmpty()) {
        onNoOverlaysRemaining()
    }
}

internal fun PresetOverlayResultModule.persistResultBoundsSupport(
    id: PresetResultWindowId,
    bounds: OverlayBounds,
) {
    val active = resultWindows[id] ?: return
    if (isStandaloneResultPreset(active.presetId)) {
        ensureCanvasWindowSupport()
        return
    }
    if (active.windowState.overlayOrder != 0) {
        ensureCanvasWindowSupport()
        return
    }
    presetRepository.updateBuiltInOverride(active.presetId) { preset ->
        preset.copy(
            windowGeometry = WindowGeometry(
                x = bounds.x,
                y = bounds.y,
                width = bounds.width,
                height = bounds.height,
            ),
        )
    }
}

internal fun PresetOverlayResultModule.updateResultWindowSupport(active: ActivePresetResultWindow) {
    val rendered = when {
        active.windowState.isLoading -> PresetRenderedContent(
            html = loadingHtml(active.windowState.loadingStatusText),
            isRawHtmlDocument = false,
        )
        active.windowState.isError -> PresetRenderedContent(
            html = errorHtml(active.windowState.markdownText),
            isRawHtmlDocument = false,
        )
        // Keep showing loading animation while streaming hasn't produced content yet
        // (e.g. model is searching sources). Prevents blank overlay during transient states.
        active.windowState.isStreaming && active.windowState.markdownText.isBlank() -> PresetRenderedContent(
            html = loadingHtml(active.windowState.loadingStatusText),
            isRawHtmlDocument = false,
        )
        !active.runtimeState.isMarkdown -> PresetRenderedContent(plainPresetResult(active.windowState.markdownText), false)
        else -> renderer.render(active.windowState.markdownText, isDarkTheme())
    }
    val disabledActions = resultWindowDisabledActions(active.presetId, active.windowState)
    val rawLoad = if (rendered.isRawHtmlDocument) {
        rawDocumentLoad(active.windowState.isStreaming, active.runtimeState.loadedRawHtml, rendered.html)
    } else {
        null
    }
    if (rawLoad == RawDocumentLoad.DEFER || rawLoad == RawDocumentLoad.KEEP) {
        resultWindows[active.id] = active.copy(
            runtimeState = active.runtimeState.copy(disabledActions = disabledActions),
        )
        if (activeResultWindowId == null) activeResultWindowId = active.id
        return
    }
    val updated = active.copy(
        runtimeState = active.runtimeState.copy(
            isRawHtml = rendered.isRawHtmlDocument,
            loadedRawHtml = rendered.html.takeIf { rendered.isRawHtmlDocument },
            disabledActions = disabledActions,
        ),
    )
    resultWindows[active.id] = updated
    if (rendered.isRawHtmlDocument) {
        updated.window.loadHtmlContent(rendered.html, RESULT_WINDOW_BASE_URL, untrustedContent = true, audioPreview = inputAudioForDownload(updated.windowState))
    } else {
        if (active.runtimeState.isRawHtml) {
            updated.window.loadHtmlContent(
                resultHtmlBuilder.build(PresetResultHtmlSettings(isDark = isDarkTheme())),
                RESULT_WINDOW_BASE_URL,
            )
        }
        updated.window.runScript(
            "window.applyResultState(${JSONObject.quote(buildResultStatePayload(updated.id, rendered.html, updated.windowState))});",
        )
    }
    if (activeResultWindowId == null) {
        activeResultWindowId = updated.id
    }
}
