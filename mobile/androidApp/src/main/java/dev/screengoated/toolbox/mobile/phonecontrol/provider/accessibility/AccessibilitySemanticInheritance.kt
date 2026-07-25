package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.view.accessibility.AccessibilityNodeInfo

internal fun AccessibilityNodeInfo.contentForPublishedActionOwner(
    editable: Boolean,
): AccessibilityNodeContent {
    val direct = accessibilityContent(editable)
    if (editable || direct.isProtected || direct.label != null || !supportsSemanticInheritance()) {
        return direct
    }
    val scan = DescendantSemanticScan()
    scanSafeDescendantSemantics(this, depth = 0, scan)
    val inherited = inheritedAccessibilityActionLabel(
        labels = scan.labels,
        unsafeDescendant = scan.unsafeDescendant,
        traversalComplete = !scan.incomplete,
    ) ?: return direct
    return direct.copy(label = inherited)
}

internal fun inheritedAccessibilityActionLabel(
    labels: List<String>,
    unsafeDescendant: Boolean,
    traversalComplete: Boolean,
): String? {
    if (unsafeDescendant || !traversalComplete) return null
    val unique = LinkedHashSet<String>()
    labels.forEach { label ->
        label.replace(Regex("\\s+"), " ")
            .trim()
            .takeIf(String::isNotEmpty)
            ?.let(unique::add)
    }
    if (unique.isEmpty()) return null
    return buildString {
        for (label in unique.take(MAX_INHERITED_LABELS)) {
            val separator = if (isEmpty()) "" else " · "
            val available = MAX_INHERITED_TEXT - length - separator.length
            if (available <= 0) break
            append(separator)
            append(label.take(available))
        }
    }.takeIf(String::isNotEmpty)
}

private fun scanSafeDescendantSemantics(
    node: AccessibilityNodeInfo,
    depth: Int,
    scan: DescendantSemanticScan,
) {
    val childCount = node.readChildCountSafely() ?: run {
        scan.incomplete = true
        return
    }
    for (index in 0 until childCount) {
        if (scan.visited >= MAX_SCANNED_DESCENDANTS) {
            scan.incomplete = true
            return
        }
        scan.visited += 1
        val childRead = node.readChildSafely(index)
        if (childRead.failed) {
            scan.incomplete = true
            return
        }
        val child = childRead.node ?: continue
        if (!child.isVisibleToUser) continue
        val editable = child.isEditable ||
            child.supportsAction(AccessibilityNodeInfo.ACTION_SET_TEXT)
        val content = child.accessibilityContent(editable)
        if (editable || content.isProtected) {
            scan.unsafeDescendant = true
            continue
        }
        if (child.supportsSemanticBoundary()) continue
        content.label?.let(scan.labels::add)
        val descendantCount = child.readChildCountSafely() ?: run {
            scan.incomplete = true
            return
        }
        if (descendantCount == 0) continue
        if (depth + 1 >= MAX_DESCENDANT_DEPTH) {
            scan.incomplete = true
            continue
        }
        scanSafeDescendantSemantics(child, depth + 1, scan)
    }
}

internal fun accessibilityActionsSupportSemanticInheritance(actionIds: Set<Int>): Boolean =
    AccessibilityNodeInfo.ACTION_CLICK in actionIds ||
        AccessibilityNodeInfo.ACTION_LONG_CLICK in actionIds ||
        AccessibilityNodeInfo.ACTION_EXPAND in actionIds ||
        AccessibilityNodeInfo.ACTION_COLLAPSE in actionIds

private fun AccessibilityNodeInfo.supportsSemanticInheritance(): Boolean =
    accessibilityActionsSupportSemanticInheritance(actionList.map { it.id }.toSet())

private fun AccessibilityNodeInfo.supportsSemanticBoundary(): Boolean =
    supportsSemanticInheritance() ||
        supportsAction(AccessibilityNodeInfo.ACTION_SET_TEXT) ||
        supportsAction(AccessibilityNodeInfo.ACTION_DISMISS)

private data class DescendantSemanticScan(
    val labels: MutableList<String> = mutableListOf(),
    var visited: Int = 0,
    var unsafeDescendant: Boolean = false,
    var incomplete: Boolean = false,
)

private const val MAX_SCANNED_DESCENDANTS = 24
private const val MAX_DESCENDANT_DEPTH = 4
private const val MAX_INHERITED_LABELS = 3
private const val MAX_INHERITED_TEXT = 320
