package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.util.Log
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo

/**
 * Accessibility service for reading selected text from any app
 * and pasting results back into focused text fields.
 *
 * User must enable in Settings > Accessibility > Screen Goated Toolbox.
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

    /** Last text seen from a text selection change event, with timestamp. */
    @Volatile
    var lastSelectedText: String? = null
    @Volatile
    private var lastSelectedTimestamp: Long = 0

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        if (event == null) return
        if (event.eventType == AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED) {
            val text = event.text?.joinToString("") ?: return
            val from = event.fromIndex
            val to = event.toIndex
            if (from >= 0 && to > from && to <= text.length) {
                lastSelectedText = text.substring(from, to)
                lastSelectedTimestamp = System.currentTimeMillis()
                Log.d(TAG, "Selection event captured: '${lastSelectedText?.take(50)}'")
            }
        }
    }

    /** Returns selection event text only if it's recent (within 5 seconds). */
    fun getRecentSelectedEvent(): String? {
        val text = lastSelectedText ?: return null
        val age = System.currentTimeMillis() - lastSelectedTimestamp
        if (age > 5000) {
            lastSelectedText = null
            return null
        }
        return text
    }

    override fun onInterrupt() {}

    override fun onDestroy() {
        super.onDestroy()
        if (instance === this) instance = null
        Log.d(TAG, "Accessibility service destroyed")
    }

    /**
     * Read the currently selected text from ANY node in the active window.
     * Searches the entire node tree — works for WebView, TextView, EditText, etc.
     * Returns the selected text, or null if nothing is selected.
     */
    fun getSelectedText(): String? {
        // Search ALL windows, not just active — our overlay may be on top
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            // Skip our own overlay windows
            if (pkg == packageName) {
                root.recycle()
                continue
            }
            val result = findSelectedTextInTree(root)
            root.recycle()
            if (result != null) return result
        }
        return null
    }

    private fun findSelectedTextInTree(node: AccessibilityNodeInfo, depth: Int = 0): String? {
        val text = node.text?.toString()
        val start = node.textSelectionStart
        val end = node.textSelectionEnd
        val cls = node.className?.toString()?.substringAfterLast('.') ?: "?"

        // Log nodes that have text (limit depth to avoid spam)
        if (text != null && depth < 6) {
            Log.d(TAG, "${"  ".repeat(depth)}[$cls] text=${text.take(40)} sel=$start..$end")
        }

        if (text != null && start >= 0 && end > start && end <= text.length) {
            return text.substring(start, end)
        }
        // Recurse into children
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val found = findSelectedTextInTree(child, depth + 1)
            child.recycle()
            if (found != null) return found
        }
        return null
    }

    /**
     * Read clipboard text as fallback when no accessibility selection is available.
     */
    fun getClipboardText(): String? {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return null
        val clip = cm.primaryClip ?: return null
        if (clip.itemCount == 0) return null
        return clip.getItemAt(0)?.text?.toString()
    }

    /**
     * Copy text to clipboard.
     */
    fun copyToClipboard(text: String) {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
        cm.setPrimaryClip(ClipData.newPlainText("SGT Result", text))
    }

    /**
     * Perform a global COPY action to capture currently selected text into clipboard.
     * This simulates what Ctrl+C does on Windows — copies whatever is selected.
     */
    fun performGlobalCopy(): Boolean {
        // Search all windows, skip our own overlay
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) {
                root.recycle()
                continue
            }
            val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            if (focused != null) {
                val result = focused.performAction(AccessibilityNodeInfo.ACTION_COPY)
                Log.d(TAG, "performGlobalCopy on $pkg focused input: $result")
                focused.recycle()
                root.recycle()
                return result
            }
            val accFocused = root.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)
            if (accFocused != null) {
                val result = accFocused.performAction(AccessibilityNodeInfo.ACTION_COPY)
                Log.d(TAG, "performGlobalCopy on $pkg accessibility focus: $result")
                accFocused.recycle()
                root.recycle()
                return result
            }
            root.recycle()
        }
        return false
    }

    /**
     * Paste clipboard content into the currently focused text field.
     * Returns true if paste was performed.
     */
    fun pasteIntoFocusedField(): Boolean {
        // Search all windows, skip our own overlay
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) {
                root.recycle()
                continue
            }
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
        Log.d(TAG, "pasteIntoFocusedField: no editable field found in any window")
        return false
    }

    private fun findFocusedEditableNode(root: AccessibilityNodeInfo): AccessibilityNodeInfo? {
        val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return null
        if (focused.isEditable || focused.className?.toString()?.contains("EditText") == true) {
            return focused
        }
        // Check if it's a WebView text field
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

        fun getSelectedTextOrClipboard(context: Context): String? {
            val service = instance
            if (service != null) {
                // 1. Try accessibility tree scan (works for EditText, some native views)
                val treeSelected = service.getSelectedText()
                if (!treeSelected.isNullOrBlank()) {
                    Log.d(TAG, "Got text from tree scan: ${treeSelected.take(50)}")
                    return treeSelected
                }
                // 2. Try last selection event (only if recent — within 5 seconds)
                val eventSelected = service.getRecentSelectedEvent()
                if (!eventSelected.isNullOrBlank()) {
                    Log.d(TAG, "Got text from selection event: ${eventSelected.take(50)}")
                    service.lastSelectedText = null // consume it
                    return eventSelected
                }
                // 3. Try performing a global COPY to capture current selection into clipboard
                service.performGlobalCopy()
                Thread.sleep(100) // brief wait for clipboard to populate

                // 4. Read clipboard (works for WebView after copy, or user's manual copy)
                val clip = service.getClipboardText()
                if (!clip.isNullOrBlank()) {
                    Log.d(TAG, "Got text from clipboard: ${clip.take(50)}")
                    return clip
                }
                return null
            }
            // No accessibility — use clipboard directly
            val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            val clip = cm?.primaryClip ?: return null
            if (clip.itemCount == 0) return null
            return clip.getItemAt(0)?.text?.toString()
        }
    }
}
