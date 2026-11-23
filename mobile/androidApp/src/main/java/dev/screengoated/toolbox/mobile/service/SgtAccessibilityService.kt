package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityService
import android.os.Bundle
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.util.Log
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo

/**
 * Accessibility service for reading selected text from any app
 * and pasting results back into focused text fields.
 *
 * User must enable in Settings > Accessibility > Screen Goated Toolbox.
 *
 * TEXT_SELECT flow:
 * 1. Tree scan: getSelectedText() — works for EditText, reads selection directly
 * 2. eagerCaptureSelection() — invokes the platform copy action
 * 3. ClipboardReaderActivity — async Activity reads clipboard (Android 10+ restriction)
 *
 * Works on Android 10+ (API 29+).
 */
class SgtAccessibilityService : AccessibilityService() {

    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
        // Applies the event profile the current phone-control demand needs.
        distributionAccessibilityServiceConnected(this)
        Log.d(TAG, "Accessibility service connected")
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        distributionAccessibilityEvent(this, event)
    }

    override fun onInterrupt() {}

    override fun onDestroy() {
        distributionAccessibilityServiceDestroyed(this)
        super.onDestroy()
        if (instance === this) instance = null
        Log.d(TAG, "Accessibility service destroyed")
    }

    // ── Text selection capture ──────────────────────────────────────────────

    /**
     * Read selected text from ANY node in all windows (skips our overlay).
     * Works for EditText, native TextViews with selection.
     * Does NOT work for WebView content (Chrome, Google Search).
     */
    fun getSelectedText(): String? {
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { continue }
            val result = findSelectedTextInTree(root)
            if (result != null) return result
        }
        return null
    }

    /**
     * Invoke the platform copy action for the current selection.
     * Must be called BEFORE ClipboardReaderActivity steals focus and
     * dismisses the toolbar. Populates the system clipboard with the
     * full cross-element selection (works for WebView, Chrome, etc.).
     */
    fun eagerCaptureSelection() {
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { continue }
            val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                ?: root.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)
            if (focused?.performAction(AccessibilityNodeInfo.ACTION_COPY) == true) {
                Log.d(TAG, "eagerCaptureSelection ACTION_COPY on $pkg")
                return
            }
        }
        // Platform menu IDs remain stable across UI languages.
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { continue }
            if (root.findAccessibilityNodeInfosByViewId("android:id/copy").any {
                    it.performAction(AccessibilityNodeInfo.ACTION_CLICK)
                }) {
                return
            }
        }
    }

    // ── Clipboard operations ────────────────────────────────────────────────

    /**
     * Read clipboard via a TYPE_ACCESSIBILITY_OVERLAY window.
     * AccessibilityService can create these overlays, and they may have
     * clipboard access as a foreground UI component.
     */
    fun readClipboardAsync(callback: (String?) -> Unit) {
        val wm = getSystemService(Context.WINDOW_SERVICE) as? android.view.WindowManager
        if (wm == null) { callback(null); return }

        val view = android.view.View(this)
        val params = android.view.WindowManager.LayoutParams(
            1, 1,
            android.view.WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            android.graphics.PixelFormat.TRANSLUCENT,
        )
        params.gravity = android.view.Gravity.TOP or android.view.Gravity.START

        try {
            wm.addView(view, params)
            // Give the overlay a moment to be considered "foreground"
            view.postDelayed({
                var text: String? = null
                try {
                    val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                    val clip = cm?.primaryClip
                    if (clip != null && clip.itemCount > 0) {
                        text = clip.getItemAt(0)?.text?.toString()
                        Log.d(
                            TAG,
                            "readClipboardAsync via overlay: items=${clip.itemCount} " +
                                "text_present=${!text.isNullOrBlank()}",
                        )
                    }
                } catch (e: Exception) {
                    Log.d(TAG, "readClipboardAsync failed: ${e.message}")
                }
                try { wm.removeView(view) } catch (_: Exception) {}
                callback(text?.takeIf { it.isNotBlank() })
            }, 200)
        } catch (e: Exception) {
            Log.d(TAG, "readClipboardAsync overlay failed: ${e.message}")
            callback(null)
        }
    }

    /**
     * Copy text to clipboard. Called from post-processing (autoCopy).
     */
    fun copyToClipboard(text: String): Boolean {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return false
        return runCatching {
            cm.setPrimaryClip(ClipData.newPlainText("SGT Result", text))
            true
        }.getOrElse { error ->
            Log.e(TAG, "copyToClipboard failed", error)
            false
        }
    }

    fun copyImageToClipboard(pngBytes: ByteArray): Boolean {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return false
        return publishImageClipboard(this, cm, pngBytes)
    }

    // ── Paste into source app ───────────────────────────────────────────────

    /**
     * Paste clipboard content into the focused editable field in the source app.
     * Skips our own overlay windows. Used for autoPaste.
     */
    fun pasteIntoFocusedField(): Boolean {
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { continue }
            val focused = findFocusedEditableNode(root)
            if (focused != null) {
                focused.performAction(AccessibilityNodeInfo.ACTION_FOCUS)
                // Try ACTION_PASTE first — the target app (focused) can read clipboard
                // even when our accessibility service cannot on Android 12+.
                if (focused.performAction(AccessibilityNodeInfo.ACTION_PASTE)) {
                    Log.d(TAG, "pasteIntoFocusedField on $pkg: ACTION_PASTE succeeded")
                    return true
                }
                // Fall back to ACTION_SET_TEXT if clipboard is readable from our context
                val clipboardText = getClipboardText()
                if (clipboardText != null) {
                    val result = appendTextToNode(focused, clipboardText)
                    Log.d(TAG, "pasteIntoFocusedField on $pkg via SET_TEXT: $result")
                    return result
                }
                Log.d(TAG, "pasteIntoFocusedField on $pkg: ACTION_PASTE failed and clipboard unreadable")
                return false
            }
        }
        Log.d(TAG, "pasteIntoFocusedField: no editable field found in any window")
        return false
    }

    // ── Private helpers ─────────────────────────────────────────────────────

    private fun findSelectedTextInTree(node: AccessibilityNodeInfo): String? {
        val text = node.text?.toString()
        if (text != null) {
            val start = node.textSelectionStart
            val end = node.textSelectionEnd
            if (start >= 0 && end > start && end <= text.length) {
                return text.substring(start, end)
            }
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val found = findSelectedTextInTree(child)
            if (found != null) return found
        }
        return null
    }

    private fun findFocusedEditableNode(root: AccessibilityNodeInfo): AccessibilityNodeInfo? {
        root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            ?.takeIf(::isEditableCandidate)
            ?.let { return it }
        root.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)
            ?.takeIf(::isEditableCandidate)
            ?.let { return it }
        findEditableNodeInTree(root, requireFocus = true)?.let { return it }
        return findEditableNodeInTree(root, requireFocus = false)
    }

    private fun findEditableNodeInTree(
        node: AccessibilityNodeInfo,
        requireFocus: Boolean,
    ): AccessibilityNodeInfo? {
        val isFocusedCandidate = !requireFocus || node.isFocused || node.isAccessibilityFocused
        if (isFocusedCandidate && isEditableCandidate(node)) {
            return node
        }
        for (index in 0 until node.childCount) {
            val child = node.getChild(index) ?: continue
            val result = findEditableNodeInTree(child, requireFocus)
            if (result != null) {
                return result
            }
        }
        return null
    }

    private fun isEditableCandidate(node: AccessibilityNodeInfo): Boolean {
        if (node.isEditable) {
            return true
        }
        val className = node.className?.toString().orEmpty()
        return className.contains("EditText") || className.contains("WebView")
    }

    /**
     * Plain text only. coerceToText would open a content URI on the clipboard
     * with SGT's identity (including SGT's own FileProvider and persisted SAF
     * grants) and the result is written into another app's field.
     */
    private fun getClipboardText(): String? {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return null
        val clip = cm.primaryClip ?: return null
        if (clip.itemCount == 0) {
            return null
        }
        return clip.getItemAt(0)?.text?.toString()?.takeIf { it.isNotBlank() }
    }

    private fun appendTextToNode(
        node: AccessibilityNodeInfo,
        text: String,
    ): Boolean {
        val appendPlan = buildAccessibilityAppendPlan(
            existingText = node.text,
            selectionStart = node.textSelectionStart,
            selectionEnd = node.textSelectionEnd,
            appendText = text,
        )
        val setTextResult = node.performAction(
            AccessibilityNodeInfo.ACTION_SET_TEXT,
            Bundle().apply {
                putCharSequence(
                    AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                    appendPlan.updatedText,
                )
            },
        )
        if (setTextResult) {
            node.performAction(
                AccessibilityNodeInfo.ACTION_SET_SELECTION,
                Bundle().apply {
                    putInt(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT,
                        appendPlan.selectionIndex,
                    )
                    putInt(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT,
                        appendPlan.selectionIndex,
                    )
                },
            )
            return true
        }
        return appendTextViaClipboardPaste(node, text)
    }

    private fun appendTextViaClipboardPaste(
        node: AccessibilityNodeInfo,
        text: String,
    ): Boolean {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return false
        val originalClip = cm.primaryClip
        return try {
            cm.setPrimaryClip(ClipData.newPlainText("SGT Stream", text))
            node.performAction(AccessibilityNodeInfo.ACTION_PASTE)
        } finally {
            if (originalClip != null) {
                cm.setPrimaryClip(originalClip)
            } else {
                cm.clearPrimaryClip()
            }
        }
    }

    private val screenshotCapture by lazy { SgtAccessibilityScreenshotCapture(this) }

    internal fun captureScreenshot(callback: (ScreenshotCaptureResult) -> Unit) =
        screenshotCapture.captureDisplay(callback)

    internal fun captureWindowScreenshot(
        accessibilityWindowId: Int,
        callback: (ScreenshotCaptureResult) -> Unit,
    ) = screenshotCapture.captureWindow(accessibilityWindowId, callback)

    internal fun screenshotSupport(): ScreenshotCaptureSupport = screenshotCapture.support()

    companion object {
        private const val TAG = "SgtAccessibility"
        @Volatile
        var instance: SgtAccessibilityService? = null
            private set

        val isAvailable: Boolean get() = instance != null

        internal val canCaptureScreenshot: Boolean
            get() = currentScreenshotSupport().available

        internal fun currentScreenshotSupport(): ScreenshotCaptureSupport {
            val service = instance
            if (service == null) {
                return ScreenshotCaptureSupport(
                    available = false,
                    failureReason = if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                        ScreenshotCaptureFailureReason.API_TOO_OLD
                    } else {
                        ScreenshotCaptureFailureReason.SERVICE_UNAVAILABLE
                    },
                )
            }
            return service.screenshotSupport()
        }
    }
}

internal data class ScreenshotCaptureSupport(
    val available: Boolean,
    val failureReason: ScreenshotCaptureFailureReason?,
)

internal enum class ScreenshotCaptureFailureReason {
    API_TOO_OLD,
    SERVICE_UNAVAILABLE,
    CAPABILITY_MISSING,
    SECURITY_EXCEPTION,
    RATE_LIMITED,
    INVALID_TARGET,
    NO_ACCESSIBILITY_ACCESS,
    SECURE_WINDOW,
    REQUEST_FAILED,
}

internal sealed interface ScreenshotCaptureResult {
    data class Success(val bitmap: android.graphics.Bitmap) : ScreenshotCaptureResult
    data class Failure(val reason: ScreenshotCaptureFailureReason) : ScreenshotCaptureResult
}
