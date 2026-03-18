package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.graphics.Rect
import android.widget.Toast

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
            updateRuntimeState(id) { it.copy(opacityPercent = value.coerceIn(10, 100)) }
            setActiveResultWindow(id)
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

internal fun PresetOverlayResultModule.canvasWindowLayoutSupport(active: ActivePresetResultWindow): PresetCanvasWindowLayout {
    return canvasWindowLayoutSupport(
        resultBounds = active.window.currentBounds(),
        screenBounds = screenBoundsProvider(),
        buttonCount = visibleCanvasButtonCountSupport(),
        dp = dp,
        cssToPhysical = cssToPhysical,
    )
}
