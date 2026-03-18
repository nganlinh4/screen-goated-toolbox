package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.Intent
import android.graphics.Rect
import android.widget.Toast
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsPriority
import dev.screengoated.toolbox.mobile.service.tts.TtsRequest
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestMode

internal fun PresetOverlayResultModule.refreshCanvasWindowForPreferencesSupport() {
    val window = canvasWindow ?: return
    window.loadHtmlContent(
        buttonCanvasHtmlBuilder.build(
            PresetButtonCanvasHtmlSettings(
                lang = uiLanguage(),
                isDark = isDarkTheme(),
            ),
        ),
        CANVAS_WINDOW_BASE_URL,
    )
    ensureCanvasWindowSupport()
}

internal fun PresetOverlayResultModule.handleCanvasMessageSupport(message: String) {
    val payload = message.jsonOrNull() ?: return
    when (payload.optString("action")) {
        "update_clickable_regions" -> {
            val regions = payload.optJSONArray("regions") ?: return
            canvasWindow?.updateTouchRegions(
                List(regions.length()) { index ->
                    val region = regions.getJSONObject(index)
                    Rect(
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
            if (window.runtimeState.isBrowsing && window.runtimeState.navDepth <= 1) {
                restoreOriginalResultSurfaceSupport(window.id)
            } else {
                window.window.goBack()
            }
            setActiveResultWindow(window.id)
        }
        "forward" -> {
            val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
            window.window.goForward()
            setActiveResultWindow(window.id)
        }
        "set_opacity" -> {
            val value = payload.optInt("value", 100)
            val id = payload.optString("hwnd").toResultWindowIdOrNull() ?: return
            val clamped = value.coerceIn(10, 100)
            val active = resultWindows[id] ?: return
            resultWindows[id] = active.copy(runtimeState = active.runtimeState.copy(opacityPercent = clamped))
            applyWindowOpacity(active.window, clamped)
            if (activeResultWindowId != id) {
                activeResultWindowId = id
            }
        }
        "canvas_content_size" -> {
            val w = payload.optInt("w", 0)
            val h = payload.optInt("h", 0)
            if (w > 0 && h > 0) {
                val canvas = canvasWindow ?: return
                val current = canvas.currentBounds()
                if (h > current.height || w > current.width) {
                    canvas.updateBounds(current.copy(
                        width = maxOf(current.width, w),
                        height = maxOf(current.height, h),
                    ))
                }
            }
        }
        "speaker" -> {
            val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
            val tts = ttsRuntimeService ?: return
            if (window.runtimeState.ttsRequestId != 0L) {
                tts.stop()
                updateRuntimeState(window.id) { it.copy(ttsRequestId = 0L, ttsLoading = false) }
            } else {
                val text = window.windowState.markdownText.trim()
                if (text.isEmpty()) return
                val snapshot = ttsSettingsSnapshotProvider?.invoke() ?: return
                val requestId = tts.enqueue(
                    TtsRequest(
                        text = text,
                        consumer = TtsConsumer.RESULT_OVERLAY,
                        priority = TtsPriority.USER,
                        requestMode = TtsRequestMode.NORMAL,
                        settingsSnapshot = snapshot,
                        ownerToken = window.id.wireValue(),
                    ),
                )
                updateRuntimeState(window.id) { it.copy(ttsRequestId = requestId, ttsLoading = true) }
            }
            setActiveResultWindow(window.id)
        }
        "edit" -> {
            val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
            val nowEditing = !window.runtimeState.isEditing
            updateRuntimeState(window.id) { it.copy(isEditing = nowEditing) }
            canvasWindow?.setFocusable(nowEditing)
            setActiveResultWindow(window.id)
        }
        "submit_refine" -> {
            val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
            val refineText = payload.optString("text").trim()
            if (refineText.isEmpty()) return
            val currentText = window.windowState.markdownText
            val windowId = window.id

            canvasWindow?.setFocusable(false)
            updateRuntimeState(windowId) { runtime ->
                runtime.copy(
                    isEditing = false,
                    textHistory = runtime.textHistory + currentText,
                    redoHistory = emptyList(),
                )
            }
            val resolved = presetRepository.getResolvedPreset(window.presetId) ?: return

            val loadingState = window.windowState.copy(
                markdownText = "",
                isLoading = true,
                isStreaming = false,
                isError = false,
                loadingStatusText = overlayLocalized(uiLanguage(), "Refining...", "Đang tinh chỉnh...", "다듬는 중..."),
            )
            resultWindows[windowId] = resultWindows[windowId]!!.copy(windowState = loadingState)
            updateResultWindowSupport(resultWindows[windowId]!!)
            val accumulated = StringBuilder()
            presetRepository.refineInPlace(
                preset = resolved.preset,
                previousText = currentText,
                refinePrompt = refineText,
                onChunk = { chunk ->

                    if (chunk.startsWith("\u0000WIPE\u0000")) {
                        accumulated.clear()
                        accumulated.append(chunk.removePrefix("\u0000WIPE\u0000"))
                    } else {
                        accumulated.append(chunk)
                    }
                    val html = accumulated.toString()
                    val active = resultWindows[windowId] ?: return@refineInPlace
                    val streamState = active.windowState.copy(
                        markdownText = html,
                        isLoading = false,
                        isStreaming = true,
                        isError = false,
                        loadingStatusText = null,
                    )
                    resultWindows[windowId] = active.copy(windowState = streamState)

                    updateResultWindowSupport(resultWindows[windowId] ?: return@refineInPlace)
                },
                onComplete = { result ->
                    val active = resultWindows[windowId] ?: return@refineInPlace
                    val finalText = result.getOrElse { it.message ?: "Refine failed" }

                    val finalState = active.windowState.copy(
                        markdownText = finalText,
                        isLoading = false,
                        isStreaming = false,
                        isError = result.isFailure,
                        loadingStatusText = null,
                    )
                    resultWindows[windowId] = active.copy(windowState = finalState)
                    updateResultWindowSupport(resultWindows[windowId]!!)
                    ensureCanvasWindowSupport()
                },
            )
            setActiveResultWindow(windowId)
        }
        "cancel_refine" -> {
            val id = payload.optString("hwnd").toResultWindowIdOrNull() ?: return
            canvasWindow?.setFocusable(false)
            updateRuntimeState(id) { it.copy(isEditing = false) }
            setActiveResultWindow(id)
        }
        "undo" -> {
            val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
            val history = window.runtimeState.textHistory
            if (history.isEmpty()) return
            val previousText = history.last()
            val currentText = window.windowState.markdownText
            updateRuntimeState(window.id) { runtime ->
                runtime.copy(
                    textHistory = runtime.textHistory.dropLast(1),
                    redoHistory = runtime.redoHistory + currentText,
                    isBrowsing = false,
                    navDepth = 0,
                    maxNavDepth = 0,
                )
            }
            val updated = resultWindows[window.id] ?: return
            val newWindowState = updated.windowState.copy(markdownText = previousText, isStreaming = false)
            resultWindows[window.id] = updated.copy(windowState = newWindowState)
            updateResultWindowSupport(resultWindows[window.id] ?: return)
            setActiveResultWindow(window.id)
        }
        "redo" -> {
            val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
            val redo = window.runtimeState.redoHistory
            if (redo.isEmpty()) return
            val nextText = redo.last()
            val currentText = window.windowState.markdownText
            updateRuntimeState(window.id) { runtime ->
                runtime.copy(
                    textHistory = runtime.textHistory + currentText,
                    redoHistory = runtime.redoHistory.dropLast(1),
                    isBrowsing = false,
                    navDepth = 0,
                    maxNavDepth = 0,
                )
            }
            val updated = resultWindows[window.id] ?: return
            val newWindowState = updated.windowState.copy(markdownText = nextText, isStreaming = false)
            resultWindows[window.id] = updated.copy(windowState = newWindowState)
            updateResultWindowSupport(resultWindows[window.id] ?: return)
            setActiveResultWindow(window.id)
        }
        "download" -> {
            val window = payload.optString("hwnd").toResultWindowIdOrNull()?.let(resultWindows::get) ?: return
            val text = window.windowState.markdownText.trim()
            if (text.isEmpty()) return
            val shareIntent = Intent(Intent.ACTION_SEND).apply {
                type = "text/plain"
                putExtra(Intent.EXTRA_TEXT, text)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            context.startActivity(Intent.createChooser(shareIntent, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            setActiveResultWindow(window.id)
        }
        "placeholder_action" -> showPlaceholderActionSupport(context, payload.optString("placeholder"), uiLanguage())
        "broom_drag_start" -> {
            val id = payload.optString("hwnd").toResultWindowIdOrNull() ?: return
            closeResultWindowSupport(id)
        }
    }
}

internal fun PresetOverlayResultModule.ensureCanvasWindowSupport() {
    val active = activeResultWindowId?.let(resultWindows::get)
    if (resultWindows.isEmpty() || active == null) {
        canvasWindow?.destroy()
        canvasWindow = null
        return
    }
    val layout = canvasWindowLayoutSupport(active)
    val spec = canvasWindowSpecSupport(layout.bounds)
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
                        isDark = isDarkTheme(),
                    ),
                ),
                CANVAS_WINDOW_BASE_URL,
            )
        }
    } else {
        canvasWindow?.updateBounds(spec.asBounds())
        canvasWindow?.show()
    }
    syncCanvasWindowSupport(canvasWindow ?: return, active, layout, CANVAS_LINGER_MS)
}

private fun PresetOverlayResultModule.applyWindowOpacity(window: PresetOverlayWindow, percent: Int) {
    window.setWindowAlpha(percent / 100f)
}

internal fun PresetOverlayResultModule.canvasWindowLayoutSupport(active: ActivePresetResultWindow): PresetCanvasWindowLayout {
    return canvasWindowLayoutSupport(
        resultBounds = active.window.currentBounds(),
        screenBounds = screenBoundsProvider(),
        buttonCount = visibleCanvasButtonCountSupport(),
        dp = dp,
        cssToPhysical = cssToPhysical,
    )
}
