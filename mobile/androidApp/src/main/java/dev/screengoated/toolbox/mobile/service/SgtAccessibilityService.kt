package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
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
            if (pkg == packageName) { root.recycle(); continue }
            val result = findSelectedTextInTree(root)
            root.recycle()
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
                copyNode.recycle()
                root.recycle()
                return
            }
            root.recycle()
        }
        // Fallback: ACTION_COPY on focused node (works for EditText)
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { root.recycle(); continue }
            val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            if (focused != null) {
                focused.performAction(AccessibilityNodeInfo.ACTION_COPY)
                Log.d(TAG, "eagerCaptureSelection ACTION_COPY on $pkg")
                focused.recycle()
                root.recycle()
                return
            }
            root.recycle()
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

    // ── Paste into source app ───────────────────────────────────────────────

    /**
     * Paste clipboard content into the focused editable field in the source app.
     * Skips our own overlay windows. Used for autoPaste.
     */
    fun pasteIntoFocusedField(): Boolean {
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { root.recycle(); continue }
            val focused = findFocusedEditableNode(root)
            if (focused != null) {
                val result = focused.performAction(AccessibilityNodeInfo.ACTION_PASTE)
                Log.d(TAG, "pasteIntoFocusedField on $pkg: $result")
                focused.recycle()
                root.recycle()
                return result
            }
            root.recycle()
        }
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
            child.recycle()
            if (found != null) return found
        }
        return null
    }

    private fun findNodeByText(root: AccessibilityNodeInfo, text: String): AccessibilityNodeInfo? {
        val nodes = root.findAccessibilityNodeInfosByText(text)
        for (node in nodes) {
            if (node.isClickable && node.text?.toString()?.trim().equals(text, ignoreCase = true)) {
                nodes.filter { it !== node }.forEach { it.recycle() }
                return node
            }
        }
        nodes.forEach { it.recycle() }
        return null
    }

    private fun findNodeByContentDescription(root: AccessibilityNodeInfo, desc: String): AccessibilityNodeInfo? {
        val nodes = root.findAccessibilityNodeInfosByText(desc)
        for (node in nodes) {
            if (node.isClickable && node.contentDescription?.toString()?.trim().equals(desc, ignoreCase = true)) {
                nodes.filter { it !== node }.forEach { it.recycle() }
                return node
            }
        }
        nodes.forEach { it.recycle() }
        return null
    }

    private fun findFocusedEditableNode(root: AccessibilityNodeInfo): AccessibilityNodeInfo? {
        val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return null
        if (focused.isEditable || focused.className?.toString()?.contains("EditText") == true) {
            return focused
        }
        if (focused.className?.toString()?.contains("WebView") == true) {
            return focused
        }
        focused.recycle()
        return null
    }

    companion object {
        private const val TAG = "SgtAccessibility"

        @Volatile
        var instance: SgtAccessibilityService? = null
            private set

        val isAvailable: Boolean get() = instance != null
    }
}
