package dev.screengoated.toolbox.mobile.service.preset

import android.os.Bundle
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService

/** Only the initially input-focused node in the focused application window is eligible. */
internal class AccessibilityProvisionalPasteTarget private constructor(
    private val service: SgtAccessibilityService,
    private val node: AccessibilityNodeInfo,
    override val replaceable: Boolean,
) : ProvisionalPasteTarget {
    override val identity: Any get() = node
    override fun snapshot(): PasteSnapshot? = runCatching {
        if (SgtAccessibilityService.instance !== service || !node.refresh()) return null
        val focused = focusedNode(service) ?: return null
        if (focused != node || !eligible(focused)) return null
        if (!focused.hasAction(AccessibilityNodeInfo.ACTION_SET_TEXT)) return null
        val rawText = if (focused.isShowingHintText) "" else focused.text ?: ""
        if (replaceable && rawText.isNotEmpty() && !focused.hasAction(AccessibilityNodeInfo.ACTION_SET_SELECTION)) return null
        if (rawText.length > MAX_PASTE_TEXT_UNITS) return null
        val text = rawText.toString()
        editablePasteSnapshot(text, focused.textSelectionStart, focused.textSelectionEnd)
            ?.takeIf { replaceable || it.end == it.text.length }
    }.getOrNull()

    override fun replace(expected: PasteSnapshot, replacement: PasteSnapshot): Boolean =
        mutate(expected, replacement) == PasteMutationOutcome.VERIFIED

    override fun mutate(expected: PasteSnapshot, replacement: PasteSnapshot): PasteMutationOutcome {
        var attempted = false
        return runCatching {
        if (snapshot() != expected || !replacement.collapsed) return PasteMutationOutcome.NO_EFFECT
        if (!replaceable && (replacement.end != replacement.text.length ||
                !replacement.text.startsWith(expected.text))) return PasteMutationOutcome.NO_EFFECT
        attempted = true
        if (!node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, Bundle().apply {
                putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, replacement.text)
            })) return PasteMutationOutcome.UNCERTAIN
        // SET_TEXT may move the caret. Never issue a second mutation after an unexpected effect.
        val written = snapshot() ?: return PasteMutationOutcome.UNCERTAIN
        if (written.text != replacement.text) return PasteMutationOutcome.UNCERTAIN
        if (written != replacement) {
            if (!replaceable || snapshot() != written) return PasteMutationOutcome.UNCERTAIN
            if (!node.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, Bundle().apply {
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, replacement.start)
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, replacement.end)
                })) return PasteMutationOutcome.UNCERTAIN
        }
        if (snapshot() == replacement) PasteMutationOutcome.VERIFIED else PasteMutationOutcome.UNCERTAIN
        }.getOrElse { if (attempted) PasteMutationOutcome.UNCERTAIN else PasteMutationOutcome.NO_EFFECT }
    }

    companion object {
        fun capture(service: SgtAccessibilityService?): ProvisionalPasteTarget? = runCatching {
            service ?: return null
            val node = focusedNode(service)?.takeIf(::eligible) ?: return null
            if (!node.hasAction(AccessibilityNodeInfo.ACTION_SET_TEXT)) return null
            AccessibilityProvisionalPasteTarget(service, node,
                node.hasAction(AccessibilityNodeInfo.ACTION_SET_SELECTION) ||
                    node.isShowingHintText || node.text.isNullOrEmpty())
                .takeIf { it.snapshot() != null }
        }.getOrNull()

        private fun focusedNode(service: SgtAccessibilityService): AccessibilityNodeInfo? {
            val window = service.windows.singleOrNull {
                it.isFocused && it.isActive && it.type == AccessibilityWindowInfo.TYPE_APPLICATION
            } ?: return null
            val root = window.root ?: return null
            if (!root.refresh()) return null
            if (root.packageName?.toString() == service.packageName) return null
            return root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                ?.takeIf { it.refresh() && it.isFocused && it.windowId == window.id }
        }

        private fun eligible(node: AccessibilityNodeInfo): Boolean =
            node.isEditable && node.isEnabled && node.isVisibleToUser && !node.isPassword

        private fun AccessibilityNodeInfo.hasAction(id: Int): Boolean = actionList.any { it.id == id }
    }
}
