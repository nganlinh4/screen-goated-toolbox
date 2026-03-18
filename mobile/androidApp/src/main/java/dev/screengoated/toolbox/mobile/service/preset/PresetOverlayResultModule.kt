package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Rect
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry
import org.json.JSONObject
import java.util.LinkedHashMap
import kotlin.math.roundToInt

internal class PresetOverlayResultModule(
    internal val context: Context,
    internal val windowManager: WindowManager,
    internal val presetRepository: PresetRepository,
    internal val dismissTarget: PresetOverlayDismissTarget,
    internal val resultHtmlBuilder: PresetResultHtmlBuilder,
    internal val buttonCanvasHtmlBuilder: PresetButtonCanvasHtmlBuilder,
    internal val renderer: PresetMarkdownRenderer,
    internal val uiLanguage: () -> String,
    internal val isDarkTheme: () -> Boolean,
    internal val screenBoundsProvider: () -> Rect,
    internal val dp: (Int) -> Int,
    internal val cssToPhysical: (Int) -> Int,
    internal val onRequestInputFront: () -> Unit,
    internal val onNoOverlaysRemaining: () -> Unit,
) {
    internal val clipboardManager = context.getSystemService(ClipboardManager::class.java)
    internal val resultWindows = LinkedHashMap<PresetResultWindowId, ActivePresetResultWindow>()
    internal var canvasWindow: PresetOverlayWindow? = null
    internal var activeResultWindowId: PresetResultWindowId? = null

    fun hasResults(): Boolean = resultWindows.isNotEmpty()

    fun destroy() {
        canvasWindow?.destroy()
        resultWindows.values.forEach { it.window.destroy() }
        canvasWindow = null
        resultWindows.clear()
        activeResultWindowId = null
    }

    fun resetExecution(resetRepository: Boolean = true) {
        presetRepository.cancelExecution()
        destroy()
        if (resetRepository) {
            presetRepository.resetState()
        }
    }

    fun refreshResultWindowsForTheme() {
        refreshResultWindowsForThemeSupport()
    }

    fun refreshCanvasWindowForPreferences() {
        refreshCanvasWindowForPreferencesSupport()
    }

    fun renderExecutionState(
        state: PresetExecutionState,
        activePreset: ResolvedPreset?,
    ) {
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
                        title = activePreset.preset.name(uiLanguage()),
                        markdownText = state.error,
                        isLoading = false,
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
                ensureCanvasWindowSupport()
            }
            return
        }

        syncResultWindowsSupport(windowsToRender, activePreset)
        ensureCanvasWindowSupport()
        onRequestInputFront()
    }

    fun handleResultMessage(message: String) {
        val payload = message.jsonOrNull() ?: return
        if (payload.optString("type") == "gestureDebug") {
            logResultGestureDebug(TAG, payload)
            return
        }

        val id = payload.optString("windowId").toResultWindowIdOrNull() ?: return
        val active = resultWindows[id] ?: return
        when (payload.optString("type")) {
            "activateResultWindow" -> setActiveResultWindow(id)
            "dragResultWindow" -> {
                dismissTarget.ensureShown()
                active.window.moveBy(
                    dx = payload.optDouble("dx", 0.0).roundToInt(),
                    dy = payload.optDouble("dy", 0.0).roundToInt(),
                    screenBounds = screenBoundsProvider(),
                )
                setActiveResultWindow(id)
            }
            "dragResultWindowAt" -> {
                dismissTarget.update(
                    dismissTarget.proximity(
                        x = payload.optInt("x"),
                        y = payload.optInt("y"),
                        screenBounds = screenBoundsProvider(),
                    ),
                )
            }
            "dragResultWindowEnd" -> {
                val proximity = dismissTarget.proximity(
                    x = payload.optInt("x"),
                    y = payload.optInt("y"),
                    screenBounds = screenBoundsProvider(),
                )
                dismissTarget.resetTracking()
                dismissTarget.hide()
                if (proximity >= 0.8f) {
                    closeResultWindowSupport(id)
                } else {
                    persistResultBoundsSupport(id, active.window.currentBounds())
                    ensureCanvasWindowSupport()
                }
            }
            "resizeResultWindow" -> {
                resizeResultWindowSupport(
                    active = active,
                    corner = payload.optString("corner"),
                    dx = payload.optDouble("dx", 0.0).roundToInt(),
                    dy = payload.optDouble("dy", 0.0).roundToInt(),
                    screenBounds = screenBoundsProvider(),
                    dp = dp,
                )
                setActiveResultWindow(id)
                ensureCanvasWindowSupport()
            }
            "resizeResultWindowEnd" -> {
                persistResultBoundsSupport(id, active.window.currentBounds())
                ensureCanvasWindowSupport()
            }
            "cancelResultGesture" -> {
                dismissTarget.resetTracking()
                dismissTarget.hide()
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
            "copySelectedText" -> {
                val text = payload.optString("text").trim()
                if (text.isNotEmpty()) {
                    clipboardManager.setPrimaryClip(
                        ClipData.newPlainText("preset_result_selection", text),
                    )
                    Toast.makeText(context, copyStatusText(), Toast.LENGTH_SHORT).show()
                }
                setActiveResultWindow(id)
            }
        }
    }

    fun handleCanvasMessage(message: String) {
        handleCanvasMessageSupport(message)
    }

    internal fun updateRuntimeState(
        id: PresetResultWindowId,
        transform: (PresetResultWindowRuntimeState) -> PresetResultWindowRuntimeState,
    ) {
        val active = resultWindows[id] ?: return
        resultWindows[id] = active.copy(runtimeState = transform(active.runtimeState))
        ensureCanvasWindowSupport()
    }

    internal fun setActiveResultWindow(id: PresetResultWindowId) {
        if (resultWindows.containsKey(id)) {
            activeResultWindowId = id
            ensureCanvasWindowSupport()
        }
    }

    internal fun handleResultPageFinished(id: PresetResultWindowId, url: String?) {
        handleResultPageFinishedSupport(id, url)
    }

    internal fun handleResultNavigationFailure(
        id: PresetResultWindowId,
        failure: OverlayNavigationFailure,
    ) {
        handleResultNavigationFailureSupport(id, failure)
    }

    internal fun loadingStatusText(): String =
        overlayLocalized(uiLanguage(), "Loading", "Đang tải", "로딩")

    internal fun copyStatusText(): String =
        overlayLocalized(uiLanguage(), "Copied", "Đã sao chép", "복사됨")

    internal companion object {
        internal const val TAG = "PresetOverlay"
    }
}
