package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds

internal data class AccessibilityElement(
    val id: Int,
    val role: String,
    val label: String?,
    val value: String?,
    val hint: String?,
    val stateDescription: String?,
    val viewId: String?,
    val packageName: String,
    val className: String?,
    val bounds: TargetBounds,
    val actions: Set<String>,
    val enabled: Boolean,
    val visible: Boolean,
    val focused: Boolean,
    val selected: Boolean,
    val checked: Boolean?,
    val isProtected: Boolean = false,
    val controllerOwned: Boolean,
    val target: PhoneControlTargetIdentity,
    val targetAuthority: AccessibilityTargetAuthority = AccessibilityTargetAuthority.ROUTINE,
) {
    fun toModelLine(): String {
        val attributes = buildList {
            add("role=$role")
            if (!isProtected) {
                label?.takeIf(String::isNotBlank)?.let { add("label=${it.compact()}" ) }
                value?.takeIf(String::isNotBlank)?.let { add("value=${it.compact()}" ) }
                hint?.takeIf(String::isNotBlank)?.let { add("hint=${it.compact()}" ) }
                stateDescription?.takeIf(String::isNotBlank)?.let { add("state=${it.compact()}" ) }
            }
            viewId?.substringAfterLast('/')?.takeIf(String::isNotBlank)?.let { add("view=$it") }
            add("bounds=${bounds.left},${bounds.top},${bounds.right},${bounds.bottom}")
            if (isProtected) add("privacy=protected")
            if (!isProtected) {
                if (!enabled) add("disabled")
                if (focused) add("focused")
                if (selected) add("selected")
                checked?.let { add("checked=$it") }
                if (controllerOwned) add("controller-owned")
                if (targetAuthority != AccessibilityTargetAuthority.ROUTINE) {
                    add("authority=${targetAuthority.wireName}")
                }
            }
            if (actions.isNotEmpty()) add("actions=${actions.sorted().joinToString("|")}")
        }
        return "@$id ${attributes.joinToString(" ")}"
    }
}

internal data class AccessibilityNodeContent(
    val label: String?,
    val value: String?,
    val hint: String?,
    val stateDescription: String?,
    val isProtected: Boolean,
) {
    val semanticFingerprintHash: Int
        get() = if (isProtected) {
            PROTECTED_SEMANTIC_FINGERPRINT
        } else {
            listOf(label, value, hint, stateDescription).hashCode()
        }
}

internal fun accessibilityNodeContent(
    isPassword: Boolean,
    contentDescription: String?,
    text: String?,
    hint: String?,
    stateDescription: String?,
    editable: Boolean,
): AccessibilityNodeContent {
    val safeDescription = contentDescription.nonBlankOrNull()
    val visibleText = text.nonBlankOrNull()
    return AccessibilityNodeContent(
        label = (safeDescription ?: visibleText).takeUnless { isPassword },
        value = visibleText.takeIf { editable && !isPassword },
        hint = hint.nonBlankOrNull().takeUnless { isPassword },
        stateDescription = stateDescription.nonBlankOrNull().takeUnless { isPassword },
        isProtected = isPassword,
    )
}

internal data class AccessibilityWindowSnapshot(
    val id: Int,
    val displayId: Int,
    val layer: Int,
    val type: String,
    val title: String?,
    val packageName: String?,
    val active: Boolean,
    val focused: Boolean,
    val bounds: TargetBounds,
    val contentAccessible: Boolean = true,
    val controllerOwned: Boolean = false,
    val pictureInPicture: Boolean = false,
    val targetAuthority: AccessibilityTargetAuthority = AccessibilityTargetAuthority.ROUTINE,
) {
    init {
        require(displayId >= 0) { "display id must be non-negative" }
    }
}

internal data class AccessibilityObservation(
    val generation: Long,
    val observedAtMs: Long,
    val displayRotation: Int,
    val densityDpi: Int,
    val windows: List<AccessibilityWindowSnapshot>,
    val elements: List<AccessibilityElement>,
    val truncated: Boolean,
) {
    fun toModelText(): String = buildString {
        append("observation_generation=").append(generation)
        append(" rotation=").append(displayRotation)
        append(" density_dpi=").append(densityDpi)
        append(" windows=").append(windows.size)
        append(" elements=").append(elements.size)
        if (truncated) append(" truncated=true")
        append('\n')
        elements.forEach { append(it.toModelLine()).append('\n') }
    }.trimEnd()
}

internal enum class AccessibilityActionVerb {
    CLICK,
    ACTIVATE,
    FILL,
    SELECT,
    SUBMIT,
    TOGGLE,
}

internal data class AccessibilityActionOutcome(
    val code: String,
    val generation: Long,
    val effect: EffectCertainty,
    val snapshotInvalidated: Boolean,
    val freshObservationRequired: Boolean,
    val message: String? = null,
)

internal data class AccessibilityGestureOutcome(
    val code: String,
    val generation: Long,
    val effect: EffectCertainty,
    val snapshotInvalidated: Boolean,
    val message: String? = null,
)

private fun String.compact(): String = replace(Regex("\\s+"), " ").trim().take(MAX_MODEL_TEXT)

private fun String?.nonBlankOrNull(): String? = this?.takeIf(String::isNotBlank)

private const val MAX_MODEL_TEXT = 320
private const val PROTECTED_SEMANTIC_FINGERPRINT = 0x50524F54
