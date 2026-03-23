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
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestSettingsSnapshot
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
    internal val onDismissAll: () -> Unit,
    internal val onNoOverlaysRemaining: () -> Unit,
    internal val onMicRequested: () -> Unit,
    internal val ttsRuntimeService: TtsRuntimeService? = null,
    internal val ttsSettingsSnapshotProvider: (() -> TtsRequestSettingsSnapshot)? = null,
    internal val overlayOpacityProvider: () -> Int = { 100 },
) {
    internal val clipboardManager = context.getSystemService(ClipboardManager::class.java)
    internal val resultWindows = LinkedHashMap<PresetResultWindowId, ActivePresetResultWindow>()
    internal var canvasWindow: PresetOverlayWindow? = null
    internal var activeResultWindowId: PresetResultWindowId? = null
    internal var topmostResultWindowId: PresetResultWindowId? = null
    internal var canvasSuspendedForGesture: Boolean = false

    fun hasResults(): Boolean = resultWindows.isNotEmpty()

    fun destroy() {
        resultWindows.values.forEach { active ->
            if (active.runtimeState.ttsRequestId != 0L) {
                ttsRuntimeService?.stopIfActive(active.runtimeState.ttsRequestId)
            }
            active.window.destroy()
        }
        canvasWindow?.destroy()
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

    fun setSuppressed(suppressed: Boolean) {
        resultWindows.values.forEach { active ->
            active.window.setSuppressed(suppressed)
        }
        canvasWindow?.setSuppressed(suppressed)
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

    fun showStandaloneMarkdownWindow(windowState: PresetResultWindowState) {
        syncStandaloneResultWindowsSupport(listOf(windowState))
        ensureCanvasWindowSupport()
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
            "promoteResultWindow" -> promoteResultWindow(id)
            "dragResultWindow" -> {
                dismissTarget.ensureShown()
                suspendCanvasForGesture()
                active.window.moveBy(
                    dx = payload.optDouble("dx", 0.0).roundToInt(),
                    dy = payload.optDouble("dy", 0.0).roundToInt(),
                    screenBounds = screenBoundsProvider(),
                )
                if (activeResultWindowId != id) {
                    activeResultWindowId = id
                }
            }
            "dragResultWindowAt" -> {
                dismissTarget.update(
                    dismissTarget.hit(
                        x = payload.optInt("x"),
                        y = payload.optInt("y"),
                        screenBounds = screenBoundsProvider(),
                    ),
                )
            }
            "dragResultWindowEnd" -> {
                val hit = dismissTarget.hit(
                    x = payload.optInt("x"),
                    y = payload.optInt("y"),
                    screenBounds = screenBoundsProvider(),
                )
                dismissTarget.resetTracking()
                dismissTarget.hide()
                when {
                    hit.allProximity >= 0.8f -> onDismissAll()
                    hit.singleProximity >= 0.8f -> closeResultWindowSupport(id)
                    else -> {
                        persistResultBoundsSupport(id, active.window.currentBounds())
                        promoteResultWindow(id)
                        resumeCanvasAfterGesture()
                    }
                }
            }
            "resizeResultWindow" -> {
                suspendCanvasForGesture()
                resizeResultWindowSupport(
                    active = active,
                    corner = payload.optString("corner"),
                    dx = payload.optDouble("dx", 0.0).roundToInt(),
                    dy = payload.optDouble("dy", 0.0).roundToInt(),
                    screenBounds = screenBoundsProvider(),
                    dp = dp,
                )
                if (activeResultWindowId != id) {
                    activeResultWindowId = id
                }
            }
            "resizeResultWindowEnd" -> {
                active.window.runScript("""
                    (function() {
                        window._streamRenderCount = 0;
                        var html = document.body.innerHTML;
                        if (typeof window.applyResultState === 'function' && typeof activeWindowId !== 'undefined') {
                            window.applyResultState(JSON.stringify({
                                windowId: activeWindowId,
                                html: html,
                                streaming: false,
                                loading: false,
                                sourceTextLen: html.length,
                                sourceTrimmedLen: html.length
                            }));
                        }
                    })();
                """.trimIndent())
                persistResultBoundsSupport(id, active.window.currentBounds())
                promoteResultWindow(id)
                resumeCanvasAfterGesture()
            }
            "cancelResultGesture" -> {
                dismissTarget.resetTracking()
                dismissTarget.hide()
                resumeCanvasAfterGesture()
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
        if (!resultWindows.containsKey(id)) return
        val activeChanged = activeResultWindowId != id
        val shouldRestoreCanvas = canvasWindow == null && !canvasSuspendedForGesture
        activeResultWindowId = id
        if (!activeChanged && !shouldRestoreCanvas) {
            return
        }
        ensureCanvasWindowSupport()
    }

    internal fun promoteResultWindow(id: PresetResultWindowId) {
        val active = resultWindows[id] ?: return
        val activeChanged = activeResultWindowId != id
        activeResultWindowId = id
        if (resultWindows.size <= 1) {
            topmostResultWindowId = id
            if (activeChanged || (canvasWindow == null && !canvasSuspendedForGesture)) {
                ensureCanvasWindowSupport()
            }
            return
        }
        val hadCanvasWindow = canvasWindow != null
        val shouldPromoteResult = topmostResultWindowId != id
        if (shouldPromoteResult) {
            active.window.bringToFront()
            topmostResultWindowId = id
        }
        if (shouldPromoteResult || activeChanged || !hadCanvasWindow) {
            ensureCanvasWindowSupport()
        }
        if (shouldPromoteResult && hadCanvasWindow && canvasWindow != null) {
            canvasWindow?.bringToFront()
        }
    }

    fun handleTtsRuntimeStateChanged(isPlaying: Boolean, activeRequestId: Long?) {
        if (activeRequestId == null) return
        resultWindows.values.forEach { active ->
            if (active.runtimeState.ttsRequestId == activeRequestId && active.runtimeState.ttsLoading && isPlaying) {
                updateRuntimeState(active.id) { it.copy(ttsLoading = false) }
            }
        }
    }

    fun handleTtsPlaybackEvent(
        requestId: Long,
        ownerToken: String,
        completionStatus: dev.screengoated.toolbox.mobile.service.tts.TtsCompletionStatus,
    ) {
        val windowId = ownerToken.toResultWindowIdOrNull() ?: return
        val window = resultWindows[windowId] ?: return
        if (window.runtimeState.ttsRequestId == requestId) {
            updateRuntimeState(windowId) { it.copy(ttsRequestId = 0L, ttsLoading = false) }
            if (completionStatus == dev.screengoated.toolbox.mobile.service.tts.TtsCompletionStatus.FAILED) {
                android.widget.Toast.makeText(
                    context,
                    overlayLocalized(uiLanguage(), "TTS failed. Check TTS settings.", "TTS thất bại. Kiểm tra cài đặt.", "TTS 실패. 설정을 확인하세요."),
                    android.widget.Toast.LENGTH_SHORT,
                ).show()
            }
        }
    }

    internal fun handleResultPageFinished(id: PresetResultWindowId, url: String?) {
        val active = resultWindows[id]
        if (active != null && !active.runtimeState.isRawHtml) {
            val cacheDir = resultHtmlBuilder.m3eCacheDir
            val libPath = cacheDir.resolve("m3e_loading_indicator.js").absolutePath
            val initPath = cacheDir.resolve("m3e_loading_init.js").absolutePath
            active.window.runScript(
                """
                (function(){
                    if(window._m3eLoaded) return;
                    window._m3eLoaded = true;
                    var s1 = document.createElement('script');
                    s1.src = 'file://$libPath';
                    s1.onload = function(){
                        var s2 = document.createElement('script');
                        s2.src = 'file://$initPath';
                        s2.onload = function(){
                            var c = document.getElementById('sgt-m3e-canvas');
                            if (c && window.initM3ELoading) {
                                window.initM3ELoading(c, {size:36, isDark:true, showContainer:false});
                            }
                        };
                        document.head.appendChild(s2);
                    };
                    document.head.appendChild(s1);
                })();
                """.trimIndent(),
            )
        }
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
