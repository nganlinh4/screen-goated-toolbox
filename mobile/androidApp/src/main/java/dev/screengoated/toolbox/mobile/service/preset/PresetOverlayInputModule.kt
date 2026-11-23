package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import kotlin.math.roundToInt

internal class PresetOverlayInputModule(
    private val context: Context,
    private val windowManager: WindowManager,
    private val textInputHtmlBuilder: PresetTextInputHtmlBuilder,
    private val dismissTarget: dev.screengoated.toolbox.mobile.service.DismissBubbleController,
    private val uiLanguage: () -> String,
    private val isDarkTheme: () -> Boolean,
    private val screenBoundsProvider: () -> Rect,
    private val dp: (Int) -> Int,
    private val onSubmit: (TextInputSubmission) -> Unit,
    private val onDismissAll: () -> Unit,
    private val onInputClosedWithoutResults: () -> Unit,
    private val hasResults: () -> Boolean,
    private val onMicRequested: () -> Unit,
    private val onWindowClosed: (String) -> Unit,
) {
    private var inputWindow: PresetOverlayWindow? = null
    private val documents = TextInputDocumentScope()
    private var activePreset: ResolvedPreset? = null
    private val inputHistory = mutableListOf<String>()
    private var historyNavigationIndex: Int? = null
    private var historyDraftText: String = ""

    fun hasWindow(): Boolean = inputWindow != null

    fun currentWindowId(): String? = documents.activeOwner

    fun currentWindow(): PresetOverlayWindow? = inputWindow

    fun open(resolvedPreset: ResolvedPreset) {
        close()
        val owner = java.util.UUID.randomUUID().toString()
        documents.open(owner)
        activePreset = resolvedPreset
        historyNavigationIndex = null
        historyDraftText = ""
        try {
            val spec = inputWindowSpecSupport(
                htmlContent = buildInputHtml(resolvedPreset, owner),
                screenBounds = screenBoundsProvider(),
                dp = dp,
            )
            val window = PresetOverlayWindow(
                context = context,
                windowManager = windowManager,
                spec = spec,
                onMessage = message@{ raw ->
                    val envelope = parseTextInputEnvelope(raw) ?: return@message
                    if (documents.accepts(owner, envelope)) handleMessage(envelope.body, owner)
                },
                onOutsideTouch = {
                    if (documents.activeOwner == owner) inputWindow?.setFocusable(false)
                },
            )
            inputWindow = window
            window.show()
            window.bringToFront(refocusIme = true)
            window.runScript("window.playEntry(); window.focusEditor(${scriptString(owner)});")
        } catch (error: Throwable) {
            runCatching { close() }.exceptionOrNull()?.let(error::addSuppressed)
            throw error
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
        documents.retire()?.let(onWindowClosed)
        dismissTarget.hide()
        inputWindow?.destroy()
        inputWindow = null
    }

    fun injectText(text: String) {
        val owner = documents.activeOwner ?: return
        inputWindow?.runScript("window.insertTextAtCursor(${scriptString(owner)}, ${scriptString(text)});")
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
        val owner = documents.activeOwner ?: return
        window.runScript(textInputHtmlBuilder.chromeScript(inputSettings(preset, owner)))
    }

    fun recordSubmittedText(submission: TextInputSubmission) {
        if (documents.activeOwner != submission.owner) return
        inputHistory.add(submission.text)
        historyNavigationIndex = null
        historyDraftText = ""
        if (activePreset?.preset?.continuousInput == true) {
            inputWindow?.runScript("window.clearInput(${scriptString(submission.owner)}, ${scriptString(submission.revision)});")
        } else {
            requestClose(animate = true)
        }
    }

    private fun handleMessage(message: String, owner: String) {
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
                onMicRequested()
            }
            message == "request_focus" -> {
                inputWindow?.setFocusable(true)
                inputWindow?.bringToFront(refocusIme = true)
            }
            message.startsWith("submit:") -> {
                parseTextInputSubmission(owner, message)?.let(onSubmit)
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
        val owner = documents.startClosing() ?: return
        onWindowClosed(owner)
        window.runScript("window.closeWithAnimation(${scriptString(owner)});")
    }

    private fun finalizeClose() {
        close()
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
        val owner = documents.activeOwner ?: return
        inputWindow?.runScript("window.setEditorText(${scriptString(owner)}, ${scriptString(nextText)});")
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

    private fun inputSettings(preset: ResolvedPreset, owner: String) = PresetTextInputHtmlSettings(
        lang = uiLanguage(), title = preset.preset.name(uiLanguage()),
        placeholder = overlayLocalized(uiLanguage(), "Type here...", "Nhập tại đây...", "여기에 입력하세요..."),
        isDark = isDarkTheme(), owner = owner,
    )

    private fun buildInputHtml(preset: ResolvedPreset, owner: String): String = textInputHtmlBuilder.build(inputSettings(preset, owner))
}
