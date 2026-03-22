package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import kotlin.math.roundToInt
import org.json.JSONObject

internal class PresetOverlayInputModule(
    private val context: Context,
    private val windowManager: WindowManager,
    private val textInputHtmlBuilder: PresetTextInputHtmlBuilder,
    private val dismissTarget: PresetOverlayDismissTarget,
    private val uiLanguage: () -> String,
    private val isDarkTheme: () -> Boolean,
    private val screenBoundsProvider: () -> Rect,
    private val dp: (Int) -> Int,
    private val onSubmit: (String) -> Unit,
    private val onDismissAll: () -> Unit,
    private val onInputClosedWithoutResults: () -> Unit,
    private val hasResults: () -> Boolean,
) {
    private var inputWindow: PresetOverlayWindow? = null
    private var activePreset: ResolvedPreset? = null
    private var inputClosing = false
    private val inputHistory = mutableListOf<String>()
    private var historyNavigationIndex: Int? = null
    private var historyDraftText: String = ""

    fun hasWindow(): Boolean = inputWindow != null

    fun currentWindow(): PresetOverlayWindow? = inputWindow

    fun open(resolvedPreset: ResolvedPreset) {
        close()
        activePreset = resolvedPreset
        inputClosing = false
        historyNavigationIndex = null
        historyDraftText = ""
        val spec = inputWindowSpecSupport(
            htmlContent = buildInputHtml(resolvedPreset),
            screenBounds = screenBoundsProvider(),
            dp = dp,
        )
        inputWindow = PresetOverlayWindow(
            context = context,
            windowManager = windowManager,
            spec = spec,
            onMessage = ::handleMessage,
        ).also { window ->
            window.show()
            window.bringToFront(refocusIme = true)
            window.runScript("window.playEntry(); window.focusEditor();")
        }
    }

    fun destroy() {
        close()
        activePreset = null
        inputHistory.clear()
        historyNavigationIndex = null
        historyDraftText = ""
    }

    fun close() {
        dismissTarget.hide()
        inputClosing = false
        inputWindow?.destroy()
        inputWindow = null
    }

    fun bringToFront() {
        inputWindow?.bringToFront(refocusIme = true)
    }

    fun setSuppressed(suppressed: Boolean) {
        inputWindow?.setSuppressed(suppressed)
    }

    fun refreshForPreferences() {
        val window = inputWindow ?: return
        val preset = activePreset ?: return
        window.runScriptForResult("window.exportDraftState && window.exportDraftState();") { raw ->
            val draftState = parseDraftState(raw)
            window.loadHtmlContent(buildInputHtml(preset), INPUT_WINDOW_BASE_URL)
            if (draftState.isNotEmpty()) {
                window.runScript(
                    "window.restoreDraftState(${JSONObject.quote(JSONObject().put("text", draftState).toString())});",
                )
            }
            window.runScript("window.focusEditor && window.focusEditor();")
        }
    }

    fun recordSubmittedText(text: String) {
        inputHistory.add(text)
        historyNavigationIndex = null
        historyDraftText = ""
        if (activePreset?.preset?.continuousInput == true) {
            inputWindow?.runScript("window.clearInput();")
        } else {
            requestClose(animate = true)
        }
    }

    fun handleMessage(message: String) {
        when {
            message == "close_window" || message == "cancel" -> {
                requestClose(animate = true)
                if (!hasResults()) {
                    onInputClosedWithoutResults()
                }
                historyNavigationIndex = null
                historyDraftText = ""
            }
            message == "input_exit_done" -> {
                finalizeClose()
            }
            message.startsWith("dragAt:") -> {
                dismissTarget.update(
                    dismissTarget.hit(
                        rawXY = message.removePrefix("dragAt:"),
                        screenBounds = screenBoundsProvider(),
                    ),
                )
            }
            message.startsWith("dragEnd:") -> {
                val hit = dismissTarget.hit(
                    rawXY = message.removePrefix("dragEnd:"),
                    screenBounds = screenBoundsProvider(),
                )
                dismissTarget.resetTracking()
                when {
                    hit.allProximity >= 0.8f -> {
                        dismissTarget.hide()
                        onDismissAll()
                    }
                    hit.singleProximity >= 0.8f -> {
                        dismissTarget.hide()
                        close()
                        if (!hasResults()) {
                            onInputClosedWithoutResults()
                        }
                        historyNavigationIndex = null
                        historyDraftText = ""
                    }
                    else -> dismissTarget.hide()
                }
            }
            message == "mic" -> {
                Toast.makeText(
                    context,
                    placeholderReasonLabel(PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY, uiLanguage()),
                    Toast.LENGTH_SHORT,
                ).show()
            }
            message.startsWith("submit:") -> {
                val text = message.substringAfter("submit:", "").trim()
                if (text.isNotEmpty()) {
                    onSubmit(text)
                }
            }
            message.startsWith("history_up:") -> {
                val current = message.substringAfter("history_up:", "")
                navigateHistory(current = current, upwards = true)
            }
            message.startsWith("history_down:") -> {
                val current = message.substringAfter("history_down:", "")
                navigateHistory(current = current, upwards = false)
            }
            message.startsWith("{") -> {
                val payload = message.jsonOrNull() ?: return
                if (payload.optString("type") == "dragInputWindow") {
                    dismissTarget.ensureShown()
                    inputWindow?.moveBy(
                        dx = payload.optDouble("dx", 0.0).roundToInt(),
                        dy = payload.optDouble("dy", 0.0).roundToInt(),
                        screenBounds = screenBoundsProvider(),
                    )
                }
            }
        }
    }

    private fun requestClose(animate: Boolean) {
        val window = inputWindow ?: return
        if (!animate) {
            finalizeClose()
            return
        }
        if (inputClosing) {
            return
        }
        inputClosing = true
        window.runScript("window.closeWithAnimation();")
    }

    private fun finalizeClose() {
        dismissTarget.hide()
        inputClosing = false
        inputWindow?.destroy()
        inputWindow = null
    }

    private fun navigateHistory(current: String, upwards: Boolean) {
        if (inputHistory.isEmpty()) {
            return
        }
        val nextText = if (upwards) {
            navigateHistoryUp(current)
        } else {
            navigateHistoryDown(current)
        } ?: return
        inputWindow?.runScript("window.setEditorText(${JSONObject.quote(nextText)});")
    }

    private fun navigateHistoryUp(current: String): String? {
        if (historyNavigationIndex == null) {
            historyDraftText = current
        }
        val (nextIndex, nextText) = navigateHistoryUpSupport(
            inputHistory = inputHistory,
            historyNavigationIndex = historyNavigationIndex,
            current = current,
        )
        historyNavigationIndex = nextIndex
        return nextText
    }

    private fun navigateHistoryDown(current: String): String? {
        val (nextIndex, nextText) = navigateHistoryDownSupport(
            inputHistory = inputHistory,
            historyNavigationIndex = historyNavigationIndex,
            current = current,
            historyDraftText = historyDraftText,
        )
        historyNavigationIndex = nextIndex
        return nextText
    }

    private fun buildInputHtml(resolvedPreset: ResolvedPreset): String {
        return buildInputHtmlSupport(
            builder = textInputHtmlBuilder,
            resolvedPreset = resolvedPreset,
            uiLanguage = uiLanguage(),
            isDark = isDarkTheme(),
            placeholder = overlayLocalized(uiLanguage(), "Type here...", "Nhập tại đây...", "여기에 입력하세요..."),
        )
    }

    private fun parseDraftState(raw: String?): String {
        if (raw.isNullOrBlank() || raw == "null") {
            return ""
        }
        val json = raw.removeSurrounding("\"").replace("\\\\", "\\").replace("\\\"", "\"")
        return json.jsonOrNull()?.optString("text").orEmpty()
    }
}
