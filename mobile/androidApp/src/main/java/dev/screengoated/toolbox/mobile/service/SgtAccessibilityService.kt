package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.os.Bundle
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.os.Build
import android.os.SystemClock
import android.util.Log
import android.view.Display
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Toast
import androidx.core.content.FileProvider
import java.io.File
import java.util.concurrent.Executors

/**
 * Accessibility service for reading selected text from any app
 * and pasting results back into focused text fields.
 *
 * User must enable in Settings > Accessibility > Screen Goated Toolbox.
 *
 * TEXT_SELECT flow:
 * 1. Tree scan: getSelectedText() — works for EditText, reads selection directly
 * 2. eagerCaptureSelection() — clicks system "Copy" button to populate clipboard
 * 3. ClipboardReaderActivity — async Activity reads clipboard (Android 10+ restriction)
 *
 * Works on Android 10+ (API 29+).
 */
class SgtAccessibilityService : AccessibilityService() {

    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
        serviceInfo = serviceInfo.apply {
            eventTypes = AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED or
                AccessibilityEvent.TYPE_VIEW_FOCUSED
            feedbackType = AccessibilityServiceInfo.FEEDBACK_GENERIC
            flags = AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS or
                AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS or
                AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
            notificationTimeout = 100
        }
        Log.d(TAG, "Accessibility service connected")
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}

    override fun onInterrupt() {}

    override fun onDestroy() {
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
     * Click the system "Copy" button in the text selection toolbar.
     * Must be called BEFORE ClipboardReaderActivity steals focus and
     * dismisses the toolbar. Populates the system clipboard with the
     * full cross-element selection (works for WebView, Chrome, etc.).
     */
    fun eagerCaptureSelection() {
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString() ?: "?"
            val copyNode = findNodeByText(root, "Copy")
                ?: findNodeByText(root, "COPY")
                ?: findNodeByContentDescription(root, "Copy")
            if (copyNode != null) {
                val clicked = copyNode.performAction(AccessibilityNodeInfo.ACTION_CLICK)
                Log.d(TAG, "eagerCaptureSelection clicked Copy on $pkg: $clicked")
                return
            }
        }
        // Fallback: ACTION_COPY on focused node (works for EditText)
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { continue }
            val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            if (focused != null) {
                focused.performAction(AccessibilityNodeInfo.ACTION_COPY)
                Log.d(TAG, "eagerCaptureSelection ACTION_COPY on $pkg")
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
                        Log.d(TAG, "readClipboardAsync via overlay: '${text?.take(50)}'")
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
    fun copyToClipboard(text: String) {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
        cm.setPrimaryClip(ClipData.newPlainText("SGT Result", text))
    }

    fun copyImageToClipboard(pngBytes: ByteArray): Boolean {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return false
        return runCatching {
            val dir = File(cacheDir, IMAGE_CLIPBOARD_DIR).apply { mkdirs() }
            val file = File(dir, IMAGE_CLIPBOARD_FILE)
            file.writeBytes(pngBytes)
            val uri = FileProvider.getUriForFile(this, "$packageName.fileprovider", file)
            cm.setPrimaryClip(ClipData.newUri(contentResolver, "SGT Image", uri))
            true
        }.getOrElse { error ->
            Log.e(TAG, "copyImageToClipboard failed", error)
            false
        }
    }

    // ── Paste into source app ───────────────────────────────────────────────

    /**
     * Paste clipboard content into the focused editable field in the source app.
     * Skips our own overlay windows. Used for autoPaste.
     */
    fun pasteIntoFocusedField(): Boolean {
        val clipboardText = getClipboardText() ?: run {
            Log.d(TAG, "pasteIntoFocusedField: clipboard is empty")
            return false
        }
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { continue }
            val focused = findFocusedEditableNode(root)
            if (focused != null) {
                focused.performAction(AccessibilityNodeInfo.ACTION_FOCUS)
                val result = focused.performAction(AccessibilityNodeInfo.ACTION_PASTE) ||
                    appendTextToNode(focused, clipboardText)
                Log.d(TAG, "pasteIntoFocusedField on $pkg: $result")
                return result
            }
        }
        Log.d(TAG, "pasteIntoFocusedField: no editable field found in any window")
        return false
    }

    fun appendTextToFocusedField(
        text: String,
        uiLanguage: String,
    ): Boolean {
        if (text.isEmpty()) {
            return false
        }
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) {
                continue
            }
            val focused = findFocusedEditableNode(root)
            if (focused != null) {
                val result = appendTextToNode(focused, text)
                Log.d(TAG, "appendTextToFocusedField on $pkg: $result")
                return result
            }
        }
        warnNoWritableTarget(uiLanguage)
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

    private fun findNodeByText(root: AccessibilityNodeInfo, text: String): AccessibilityNodeInfo? {
        val nodes = root.findAccessibilityNodeInfosByText(text)
        for (node in nodes) {
            if (node.isClickable && node.text?.toString()?.trim().equals(text, ignoreCase = true)) {
                return node
            }
        }
        return null
    }

    private fun findNodeByContentDescription(root: AccessibilityNodeInfo, desc: String): AccessibilityNodeInfo? {
        val nodes = root.findAccessibilityNodeInfosByText(desc)
        for (node in nodes) {
            if (node.isClickable && node.contentDescription?.toString()?.trim().equals(desc, ignoreCase = true)) {
                return node
            }
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

    private fun getClipboardText(): String? {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return null
        val clip = cm.primaryClip ?: return null
        if (clip.itemCount == 0) {
            return null
        }
        return clip.getItemAt(0)?.coerceToText(this)?.toString()?.takeIf { it.isNotBlank() }
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
            } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                cm.clearPrimaryClip()
            }
        }
    }

    private fun warnNoWritableTarget(uiLanguage: String) {
        val nowMs = SystemClock.elapsedRealtime()
        if (nowMs - lastNoWritableTargetToastAtMs < NO_WRITABLE_TARGET_COOLDOWN_MS) {
            return
        }
        lastNoWritableTargetToastAtMs = nowMs
        val message = when (uiLanguage) {
            "vi" -> "Tự động gõ đang bật nhưng chưa chọn ô nhập text."
            "ko" -> "자동 입력이 켜져 있지만 텍스트 입력 칸이 선택되지 않았습니다."
            else -> "Auto writing is active, but no text field is selected."
        }
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
    }

    // ── Screenshot capture ─────────────────────────────────────────────────

    private val screenshotExecutor by lazy { Executors.newSingleThreadExecutor() }

    /**
     * Capture a screenshot via AccessibilityService.takeScreenshot() (API 30+).
     * Returns an ARGB_8888 bitmap via callback on the main thread.
     */
    internal fun captureScreenshot(callback: (ScreenshotCaptureResult) -> Unit) {
        val support = screenshotSupport()
        if (!support.available) {
            Log.d(TAG, "captureScreenshot unavailable: ${support.failureReason}")
            postScreenshotResult(
                callback,
                ScreenshotCaptureResult.Failure(
                    support.failureReason ?: ScreenshotCaptureFailureReason.REQUEST_FAILED,
                ),
            )
            return
        }
        try {
            takeScreenshot(
                Display.DEFAULT_DISPLAY,
                screenshotExecutor,
                object : TakeScreenshotCallback {
                    override fun onSuccess(result: ScreenshotResult) {
                        val callbackStartedAt = SystemClock.elapsedRealtime()
                        Log.d(TAG, "captureScreenshot callback received")
                        var capturedBitmap: Bitmap? = null
                        try {
                            val bitmap = Bitmap.wrapHardwareBuffer(
                                result.hardwareBuffer,
                                result.colorSpace,
                            )
                            if (bitmap != null) {
                                val softBitmap = bitmap.copy(Bitmap.Config.ARGB_8888, false)
                                bitmap.recycle()
                                capturedBitmap = softBitmap
                                Log.d(
                                    TAG,
                                    "captureScreenshot bitmap ready ${softBitmap.width}x${softBitmap.height} in ${SystemClock.elapsedRealtime() - callbackStartedAt}ms",
                                )
                            }
                        } catch (e: Exception) {
                            Log.e(TAG, "captureScreenshot bitmap conversion failed", e)
                        } finally {
                            result.hardwareBuffer.close()
                        }
                        postScreenshotResult(
                            callback,
                            capturedBitmap?.let(ScreenshotCaptureResult::Success)
                                ?: ScreenshotCaptureResult.Failure(ScreenshotCaptureFailureReason.REQUEST_FAILED),
                        )
                    }

                    override fun onFailure(errorCode: Int) {
                        Log.d(TAG, "captureScreenshot failed: errorCode=$errorCode")
                        postScreenshotResult(
                            callback,
                            ScreenshotCaptureResult.Failure(mapScreenshotFailure(errorCode)),
                        )
                    }
                },
            )
        } catch (e: SecurityException) {
            Log.e(TAG, "captureScreenshot security failure", e)
            postScreenshotResult(
                callback,
                ScreenshotCaptureResult.Failure(ScreenshotCaptureFailureReason.SECURITY_EXCEPTION),
            )
        }
    }

    internal fun screenshotSupport(): ScreenshotCaptureSupport {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return ScreenshotCaptureSupport(
                available = false,
                failureReason = ScreenshotCaptureFailureReason.API_TOO_OLD,
            )
        }
        val capabilities = serviceInfo?.capabilities ?: 0
        return if (capabilities and AccessibilityServiceInfo.CAPABILITY_CAN_TAKE_SCREENSHOT != 0) {
            ScreenshotCaptureSupport(available = true, failureReason = null)
        } else {
            ScreenshotCaptureSupport(
                available = false,
                failureReason = ScreenshotCaptureFailureReason.CAPABILITY_MISSING,
            )
        }
    }

    private fun postScreenshotResult(
        callback: (ScreenshotCaptureResult) -> Unit,
        result: ScreenshotCaptureResult,
    ) {
        android.os.Handler(android.os.Looper.getMainLooper()).post {
            callback(result)
        }
    }

    private fun mapScreenshotFailure(errorCode: Int): ScreenshotCaptureFailureReason {
        return when (errorCode) {
            ERROR_TAKE_SCREENSHOT_INTERVAL_TIME_SHORT -> ScreenshotCaptureFailureReason.RATE_LIMITED
            ERROR_TAKE_SCREENSHOT_INVALID_DISPLAY,
            ERROR_TAKE_SCREENSHOT_INVALID_WINDOW,
            -> ScreenshotCaptureFailureReason.INVALID_TARGET

            ERROR_TAKE_SCREENSHOT_NO_ACCESSIBILITY_ACCESS ->
                ScreenshotCaptureFailureReason.NO_ACCESSIBILITY_ACCESS

            ERROR_TAKE_SCREENSHOT_SECURE_WINDOW -> ScreenshotCaptureFailureReason.SECURE_WINDOW
            ERROR_TAKE_SCREENSHOT_INTERNAL_ERROR -> ScreenshotCaptureFailureReason.REQUEST_FAILED
            else -> ScreenshotCaptureFailureReason.REQUEST_FAILED
        }
    }

    companion object {
        private const val TAG = "SgtAccessibility"
        private const val IMAGE_CLIPBOARD_DIR = "clipboard-images"
        private const val IMAGE_CLIPBOARD_FILE = "latest-screenshot.png"
        private const val NO_WRITABLE_TARGET_COOLDOWN_MS = 3_000L
        @Volatile
        private var lastNoWritableTargetToastAtMs: Long = 0L

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
    data class Success(val bitmap: Bitmap) : ScreenshotCaptureResult
    data class Failure(val reason: ScreenshotCaptureFailureReason) : ScreenshotCaptureResult
}
