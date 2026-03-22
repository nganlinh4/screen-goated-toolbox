package dev.screengoated.toolbox.mobile.service

internal data class AccessibilityAppendPlan(
    val updatedText: CharSequence,
    val selectionIndex: Int,
)

internal fun buildAccessibilityAppendPlan(
    existingText: CharSequence?,
    selectionStart: Int,
    selectionEnd: Int,
    appendText: String,
): AccessibilityAppendPlan {
    val safeText = existingText?.toString().orEmpty()
    val safeStart = selectionStart.takeIf { it in 0..safeText.length } ?: safeText.length
    val safeEnd = selectionEnd.takeIf { it in 0..safeText.length } ?: safeStart
    val rangeStart = minOf(safeStart, safeEnd)
    val rangeEnd = maxOf(safeStart, safeEnd)
    val prefix = safeText.substring(0, rangeStart)
    val suffix = safeText.substring(rangeEnd, safeText.length)
    val combined = prefix + appendText + suffix
    return AccessibilityAppendPlan(
        updatedText = combined,
        selectionIndex = prefix.length + appendText.length,
    )
}

