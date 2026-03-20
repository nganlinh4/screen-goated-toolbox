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

    private var clipboardListener: android.content.ClipboardManager.OnPrimaryClipChangedListener? = null

    /** Last clipboard text detected via listener. Updated in real-time. */
    @Volatile
    var lastClipboardText: String? = null
        private set
    @Volatile
    private var lastClipboardTimestamp: Long = 0

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

        // Listen for clipboard changes — this works even for services
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        if (cm != null) {
            clipboardListener = android.content.ClipboardManager.OnPrimaryClipChangedListener {
                try {
                    val clip = cm.primaryClip
                    if (clip != null && clip.itemCount > 0) {
                        val text = clip.getItemAt(0)?.text?.toString()
                        if (!text.isNullOrBlank()) {
                            lastClipboardText = text
                            lastClipboardTimestamp = System.currentTimeMillis()
                            Log.d(TAG, "Clipboard changed: '${text.take(50)}'")
                        }
                    }
                } catch (e: Exception) {
                    Log.d(TAG, "Clipboard listener error: ${e.message}")
                }
            }
            cm.addPrimaryClipChangedListener(clipboardListener)
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
        clipboardListener?.let {
            (getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager)
                ?.removePrimaryClipChangedListener(it)
        }
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
    /**
     * Eagerly capture text by clicking the "Copy" button in the selection toolbar.
     * Must be called BEFORE any UI interaction dismisses the toolbar.
     * Stores result in [eagerClipboardCapture] for later retrieval.
     */
    fun eagerCaptureSelection() {
        Log.d(TAG, "eagerCaptureSelection: searching ${windows.size} windows for Copy button")
        // Clear previous capture so we can detect new clipboard change
        val clipBefore = lastClipboardText

        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString() ?: "?"

            val copyNode = findNodeByText(root, "Copy")
                ?: findNodeByText(root, "COPY")
                ?: findNodeByContentDescription(root, "Copy")
            if (copyNode != null) {
                // Use ACTION_CLICK — it reports success on the Copy button
                val clicked = copyNode.performAction(AccessibilityNodeInfo.ACTION_CLICK)
                Log.d(TAG, "eagerCaptureSelection ACTION_CLICK Copy on $pkg: $clicked")
                copyNode.recycle()
                root.recycle()

                if (clicked) {
                    // Wait for clipboard listener to fire with new content
                    for (attempt in 1..10) {
                        Thread.sleep(50)
                        if (lastClipboardText != clipBefore && !lastClipboardText.isNullOrBlank()) {
                            Log.d(TAG, "eagerCaptureSelection got new clipboard (attempt $attempt): '${lastClipboardText?.take(50)}'")
                            return
                        }
                    }
                    Log.d(TAG, "eagerCaptureSelection: clicked Copy but no new clipboard after 500ms")
                }
                return
            }
            root.recycle()
        }

        // Fallback: ACTION_COPY on focused node
        for (window in windows) {
            val root = window.root ?: continue
            val pkg = root.packageName?.toString()
            if (pkg == packageName) { root.recycle(); continue }
            val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                ?: root.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)
            if (focused != null) {
                focused.performAction(AccessibilityNodeInfo.ACTION_COPY)
                Log.d(TAG, "eagerCaptureSelection ACTION_COPY on $pkg")
                focused.recycle()
                root.recycle()
                Thread.sleep(200)
                return
            }
            root.recycle()
        }
    }

    /** Text captured by [eagerCaptureSelection], consumed on first read. */
    @Volatile
    var eagerClipboardCapture: String? = null

    private fun findNodeByText(root: AccessibilityNodeInfo, text: String): AccessibilityNodeInfo? {
        val nodes = root.findAccessibilityNodeInfosByText(text)
        for (node in nodes) {
            if (node.isClickable && node.text?.toString()?.trim().equals(text, ignoreCase = true)) {
                // Recycle other matches
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
            }

            // 2. Read clipboard via transparent Activity (only way on Android 12+)
            //    The Activity briefly gains foreground → reads clipboard → finishes
            // Check clipboard via the transparent Activity that was launched
            // by eagerCaptureSelection earlier (it runs async and stores result)
            val clipText = ClipboardReaderActivity.getRecentText()
            if (!clipText.isNullOrBlank()) {
                Log.d(TAG, "Got text from ClipboardReaderActivity: ${clipText.take(50)}")
                ClipboardReaderActivity.consume()
                return clipText
            }

            if (service != null) {
                // 3. Last resort: selection event (may be partial for WebView)
                val eventSelected = service.getRecentSelectedEvent()
                if (!eventSelected.isNullOrBlank()) {
                    Log.d(TAG, "Got text from selection event (fallback): ${eventSelected.take(50)}")
                    service.lastSelectedText = null
                    return eventSelected
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
