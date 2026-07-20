package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.accessibilityservice.InputMethod
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.accessibility.AccessibilityNodeInfo
import androidx.annotation.RequiresApi
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

internal data class AccessibilityTextTarget(
    val id: Int,
    val identity: PhoneControlTargetIdentity,
    val authority: AccessibilityTargetAuthority,
)

internal data class AccessibilityTextOutcome(
    val code: String,
    val provider: String,
    val generation: Long,
    val effect: EffectCertainty,
    val snapshotInvalidated: Boolean,
    val freshObservationRequired: Boolean,
    val insertedCodePoints: Int = 0,
    val completedKeyGroups: Int = 0,
    val submitted: Boolean = false,
    val message: String? = null,
)

internal suspend fun findFocusedAccessibilityTextTarget(
    provider: PhoneControlAccessibilityProvider,
    surface: AndroidSurfaceIdentity? = null,
): AccessibilityProviderResult<AccessibilityTextTarget> {
    val observed = when (val result = provider.observe()) {
        is AccessibilityProviderResult.Failure -> return result
        is AccessibilityProviderResult.Success -> result.value
    }
    if (surface != null) {
        val currentSurface = observed.windows.singleOrNull { window ->
            window.displayId == surface.displayId &&
                window.id.toLong() == surface.windowId &&
                window.packageName.orEmpty() == surface.packageName
        }
        if (surface.generation != observed.generation ||
            currentSurface == null || !currentSurface.active || !currentSurface.focused
        ) {
            return staleTextTarget()
        }
    }
    val focusedWindows = observed.windows.filter { it.active || it.focused }
        .map { it.displayId to it.id.toLong() }
        .toSet()
    val candidates = observed.elements.filter { element ->
        element.focused &&
            element.enabled &&
            element.visible &&
            !element.controllerOwned &&
            (element.role == "text_field" || "fill" in element.actions) &&
            (focusedWindows.isEmpty() ||
                (element.target.displayId to element.target.windowId) in focusedWindows) &&
            (surface == null || (
                element.target.displayId == surface.displayId &&
                    element.target.windowId == surface.windowId &&
                    element.target.packageOrSurface == surface.packageName
                ))
    }
    return when (candidates.size) {
        1 -> AccessibilityProviderResult.Success(
            AccessibilityTextTarget(
                candidates.single().id,
                candidates.single().target,
                candidates.single().targetAuthority,
            ),
        )
        0 -> textFailure(
            "focused_editor_unavailable",
            "No focused editable Accessibility target is available.",
            retryable = true,
            freshObservationRequired = true,
        )
        else -> textFailure(
            "focused_editor_ambiguous",
            "More than one focused editable target is visible.",
            retryable = true,
            freshObservationRequired = true,
        )
    }
}

internal suspend fun performAccessibilityTextEdit(
    provider: PhoneControlAccessibilityProvider,
    target: AccessibilityTextTarget,
    text: String,
    slow: Boolean,
    pressEnter: Boolean,
): AccessibilityProviderResult<AccessibilityTextOutcome> {
    val lease = currentTextLease(provider, target) ?: return staleTextTarget()
    if (text.isEmpty() && !pressEnter) {
        return AccessibilityProviderResult.Success(
            AccessibilityTextOutcome(
                code = "ok",
                provider = ACCESSIBILITY_PROVIDER,
                generation = provider.observationGeneration,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                freshObservationRequired = false,
            ),
        )
    }
    provider.validateTargetMutation(
        lease,
        if (pressEnter) AccessibilityMutationKind.TEXT_SUBMIT else AccessibilityMutationKind.TEXT_EDIT,
        confirmed = false,
    )?.let { return it }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        val kind = if (pressEnter) {
            AccessibilityMutationKind.TEXT_SUBMIT
        } else {
            AccessibilityMutationKind.TEXT_EDIT
        }
        when (val session = prepareInputSession(provider, lease, kind)) {
            is AccessibilityProviderResult.Success -> {
                return commitViaInputMethod(provider, lease, session.value, text, slow, pressEnter)
            }
            is AccessibilityProviderResult.Failure -> if (pressEnter) return session
        }
    } else if (pressEnter) {
        return unsupportedInputMethod()
    }
    return setTextOnFreshNode(provider, lease, text)
}

internal fun currentTextLease(
    provider: PhoneControlAccessibilityProvider,
    target: AccessibilityTextTarget,
): AccessibilityTargetLease? {
    val lease = provider.currentLease(target.id) ?: return null
    return lease.takeIf {
        it.identity == target.identity &&
            it.authority == target.authority &&
            provider.currentCaptureGeneration() == it.identity.snapshotGeneration &&
            provider.observationGeneration == it.identity.snapshotGeneration
    }
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
internal suspend fun prepareInputSession(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilityTargetLease,
    kind: AccessibilityMutationKind,
): AccessibilityProviderResult<InputSession> = provider.onServiceMain { service ->
    provider.validateTargetMutation(lease, kind, confirmed = false)
        ?.let { return@onServiceMain it }
    val node = resolveExactFocusedEditor(service, lease)
        ?: return@onServiceMain staleTextTarget()
    val inputMethod = service.inputMethod
        ?: return@onServiceMain unsupportedInputMethod()
    val editor = inputMethod.currentInputEditorInfo
        ?: return@onServiceMain unsupportedInputMethod()
    val connection = inputMethod.currentInputConnection
        ?: return@onServiceMain unsupportedInputMethod()
    if (!inputMethod.currentInputStarted || editor.packageName != lease.identity.packageOrSurface) {
        return@onServiceMain unsupportedInputMethod()
    }
    AccessibilityProviderResult.Success(
        InputSession(
            connection = connection,
            binding = InputBinding(editor.packageName, editor.fieldId),
            beforeText = node.text?.toString().takeUnless { node.isPassword },
            selectionStart = node.textSelectionStart,
            selectionEnd = node.textSelectionEnd,
        ),
    )
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
private suspend fun commitViaInputMethod(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilityTargetLease,
    session: InputSession,
    text: String,
    slow: Boolean,
    pressEnter: Boolean,
): AccessibilityProviderResult<AccessibilityTextOutcome> {
    val ownedEffect = OwnedAccessibilityEffect.begin()
    var platformSettled = false
    try {
    val chunks = if (slow) text.toCodePointStrings() else listOf(text).filter(String::isNotEmpty)
    val mutationKind = if (pressEnter) {
        AccessibilityMutationKind.TEXT_SUBMIT
    } else {
        AccessibilityMutationKind.TEXT_EDIT
    }
    var inserted = 0
    for (chunk in chunks) {
        currentCoroutineContext().ensureActive()
        val dispatch = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
            provider.validateTargetMutation(lease, mutationKind, confirmed = false)
                ?.let { return@onServiceMain it }
            if (!matchesInputBinding(service, lease, session.binding)) {
                return@onServiceMain AccessibilityProviderResult.Success(false)
            }
            val dispatched = ownedEffect.dispatch {
                session.connection.commitText(chunk, 1, null)
            }
            if (!dispatched) return@onServiceMain cancelledBeforeDispatch()
            AccessibilityProviderResult.Success(true)
        }
        when (dispatch) {
            is AccessibilityProviderResult.Failure -> {
                if (inserted == 0) return dispatch
                provider.invalidate("input_method_authority_changed")
                return dispatch.afterPartialInput("Text entry stopped after $inserted code points.")
            }
            is AccessibilityProviderResult.Success -> if (!dispatch.value) {
                if (inserted > 0) provider.invalidate("input_method_target_changed")
                return AccessibilityProviderResult.Success(
                    partialTextOutcome(
                        provider,
                        inserted,
                        "The focused editor changed or rejected text input.",
                    ),
                )
            }
        }
        inserted += chunk.codePointCount(0, chunk.length)
        if (slow) delay(SLOW_INPUT_DELAY_MS)
    }
    if (pressEnter) {
        when (val dispatched = sendAccessibilityKeyGroup(
                provider,
                lease,
                session.connection,
                session.binding,
                AccessibilityKeyGroup(listOf(KeyEvent.KEYCODE_ENTER)),
                ENTER_HOLD_MS,
                AccessibilityMutationKind.TEXT_SUBMIT,
            )) {
            is AccessibilityProviderResult.Failure -> {
                if (inserted == 0) return dispatched
                provider.invalidate("input_method_authority_changed_before_enter")
                return dispatched.afterPartialInput("Text was entered, but Enter was not dispatched.")
            }
            is AccessibilityProviderResult.Success -> if (!dispatched.value) {
                if (inserted > 0) provider.invalidate("input_method_unavailable_before_enter")
                return AccessibilityProviderResult.Success(
                    partialTextOutcome(provider, inserted, "Text was entered, but Enter was not dispatched."),
                )
            }
        }
    }
    provider.invalidate("accessibility_input_method_text")
    withContext(NonCancellable) { delay(POSTCONDITION_DELAY_MS) }
    platformSettled = true
    val verified = if (pressEnter) {
        false
    } else {
        verifyExpectedNodeText(provider, lease, session.beforeText, session.selectionStart, session.selectionEnd, text)
    }
    return AccessibilityProviderResult.Success(
        AccessibilityTextOutcome(
            code = "ok",
            provider = INPUT_METHOD_PROVIDER,
            generation = provider.observationGeneration,
            effect = if (verified) EffectCertainty.VERIFIED else EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = text.isNotEmpty() || pressEnter,
            freshObservationRequired = text.isNotEmpty() || pressEnter,
            insertedCodePoints = inserted,
            submitted = pressEnter,
        ),
    )
    } finally {
        try {
            if (ownedEffect.wasAccepted && !platformSettled) {
                withContext(NonCancellable) { delay(CANCELLED_INPUT_SETTLE_DELAY_MS) }
            }
        } finally {
            ownedEffect.close()
        }
    }
}

private suspend fun setTextOnFreshNode(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilityTargetLease,
    text: String,
): AccessibilityProviderResult<AccessibilityTextOutcome> {
    val ownedEffect = OwnedAccessibilityEffect.begin()
    var platformSettled = false
    try {
    val dispatch = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
        provider.validateTargetMutation(
            lease,
            AccessibilityMutationKind.TEXT_EDIT,
            confirmed = false,
        )?.let { return@onServiceMain it }
        val node = resolveExactFocusedEditor(service, lease)
            ?: return@onServiceMain staleTextTarget()
        if (node.isPassword || !node.supportsAction(AccessibilityNodeInfo.ACTION_SET_TEXT)) {
            return@onServiceMain exactSetTextUnavailable()
        }
        val before = node.text?.toString() ?: if (
            node.textSelectionStart == 0 && node.textSelectionEnd == 0
        ) {
            ""
        } else {
            return@onServiceMain exactSetTextUnavailable()
        }
        val start = node.textSelectionStart
        val end = node.textSelectionEnd
        if (start !in 0..before.length || end !in 0..before.length) {
            return@onServiceMain exactSetTextUnavailable()
        }
        val expected = insertAtSelection(before, start, end, text)
        val accepted = ownedEffect.dispatchBoolean {
            node.performAction(
                AccessibilityNodeInfo.ACTION_SET_TEXT,
                Bundle().apply {
                    putCharSequence(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                        expected,
                    )
                },
            )
        } ?: return@onServiceMain cancelledBeforeDispatch()
        AccessibilityProviderResult.Success(NodeTextDispatch(accepted, expected))
    }
    val accepted = when (dispatch) {
        is AccessibilityProviderResult.Failure -> return dispatch
        is AccessibilityProviderResult.Success -> dispatch.value
    }
    if (!accepted.accepted) {
        return AccessibilityProviderResult.Success(
            AccessibilityTextOutcome(
                code = "action_rejected",
                provider = ACCESSIBILITY_PROVIDER,
                generation = provider.observationGeneration,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                freshObservationRequired = false,
            ),
        )
    }
    provider.invalidate("accessibility_set_text")
    val verified = withContext(NonCancellable) {
        delay(POSTCONDITION_DELAY_MS)
        provider.onServiceMain { service ->
            val node = resolveStableFocusedEditor(service, lease)
            AccessibilityProviderResult.Success(node?.text?.toString() == accepted.expected)
        }
    }
    platformSettled = true
    return AccessibilityProviderResult.Success(
        AccessibilityTextOutcome(
            code = "ok",
            provider = ACCESSIBILITY_PROVIDER,
            generation = provider.observationGeneration,
            effect = if ((verified as? AccessibilityProviderResult.Success)?.value == true) {
                EffectCertainty.VERIFIED
            } else {
                EffectCertainty.MAY_HAVE_OCCURRED
            },
            snapshotInvalidated = true,
            freshObservationRequired = true,
            insertedCodePoints = text.codePointCount(0, text.length),
        ),
    )
    } finally {
        try {
            if (ownedEffect.wasAccepted && !platformSettled) {
                withContext(NonCancellable) { delay(POSTCONDITION_DELAY_MS) }
            }
        } finally {
            ownedEffect.close()
        }
    }
}

private fun resolveExactFocusedEditor(
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
): AccessibilityNodeInfo? = resolveTextAccessibilityNode(service, lease)?.takeIf { node ->
    node.matches(lease) && node.isFocused && writableEditor(node, service, lease)
}

private fun resolveStableFocusedEditor(
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
): AccessibilityNodeInfo? = resolveTextAccessibilityNode(service, lease)?.takeIf { node ->
    node.matchesStableTextTarget(lease) && node.isFocused && writableEditor(node, service, lease)
}

private fun writableEditor(
    node: AccessibilityNodeInfo,
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
): Boolean = !lease.accessibilityOverlay &&
    lease.identity.packageOrSurface != service.packageName &&
    node.isVisibleToUser &&
    node.isEnabled &&
    (node.isEditable || node.supportsAction(AccessibilityNodeInfo.ACTION_SET_TEXT))

private fun resolveTextAccessibilityNode(
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
): AccessibilityNodeInfo? {
    var node = findAccessibilityWindowRoot(
        service,
        lease.identity.displayId,
        lease.identity.windowId,
    ) ?: return null
    for (index in lease.childPath) {
        node = node.getChild(index) ?: return null
    }
    return node
}

private fun AccessibilityNodeInfo.matchesStableTextTarget(lease: AccessibilityTargetLease): Boolean {
    val fingerprint = lease.fingerprint
    return packageName?.toString().orEmpty().ifBlank { "unknown" } == fingerprint.packageName &&
        className?.toString() == fingerprint.className &&
        viewIdResourceName == fingerprint.viewId &&
        actionList.map { it.id }.toSet() == fingerprint.actions &&
        android.graphics.Rect().also(::getBoundsInScreen).let { bounds ->
            bounds.left == fingerprint.bounds.left &&
                bounds.top == fingerprint.bounds.top &&
                bounds.right == fingerprint.bounds.right &&
                bounds.bottom == fingerprint.bounds.bottom
        }
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
internal fun matchesInputBinding(
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
    binding: InputBinding,
): Boolean {
    val node = resolveStableFocusedEditor(service, lease) ?: return false
    val input = service.inputMethod ?: return false
    val editor = input.currentInputEditorInfo ?: return false
    return node.isFocused && input.currentInputStarted && input.currentInputConnection != null &&
        editor.packageName == binding.packageName && editor.fieldId == binding.fieldId
}

private suspend fun verifyExpectedNodeText(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilityTargetLease,
    beforeText: String?,
    selectionStart: Int,
    selectionEnd: Int,
    inserted: String,
): Boolean {
    val before = beforeText ?: return false
    if (selectionStart !in 0..before.length || selectionEnd !in 0..before.length) return false
    val expected = insertAtSelection(before, selectionStart, selectionEnd, inserted)
    val result = provider.onServiceMain { service ->
        AccessibilityProviderResult.Success(
            resolveStableFocusedEditor(service, lease)?.text?.toString() == expected,
        )
    }
    return (result as? AccessibilityProviderResult.Success)?.value == true
}

private fun insertAtSelection(text: String, start: Int, end: Int, inserted: String): String {
    val low = minOf(start, end)
    val high = maxOf(start, end)
    return text.substring(0, low) + inserted + text.substring(high)
}

private fun String.toCodePointStrings(): List<String> = buildList {
    var offset = 0
    while (offset < length) {
        val codePoint = codePointAt(offset)
        add(String(Character.toChars(codePoint)))
        offset += Character.charCount(codePoint)
    }
}

private fun partialTextOutcome(
    provider: PhoneControlAccessibilityProvider,
    inserted: Int,
    message: String,
) = AccessibilityTextOutcome(
    code = if (inserted == 0) "stale_target" else "partial",
    provider = INPUT_METHOD_PROVIDER,
    generation = provider.observationGeneration,
    effect = if (inserted == 0) EffectCertainty.PROVEN_NO_EFFECT else EffectCertainty.MAY_HAVE_OCCURRED,
    snapshotInvalidated = inserted > 0,
    freshObservationRequired = true,
    insertedCodePoints = inserted,
    message = message,
)

private fun AccessibilityProviderResult.Failure.afterPartialInput(
    partialMessage: String,
) = copy(
    message = "$partialMessage $message",
    freshObservationRequired = true,
    effect = EffectCertainty.MAY_HAVE_OCCURRED,
)

private fun staleTextTarget(): AccessibilityProviderResult.Failure = textFailure(
    "stale_target",
    "The focused editor target is stale or no longer focused.",
    retryable = true,
    freshObservationRequired = true,
)

internal fun unsupportedInputMethod(): AccessibilityProviderResult.Failure = textFailure(
    "unsupported_on_surface",
    "The focused editor has no exact Accessibility input connection.",
    retryable = false,
)

private fun exactSetTextUnavailable(): AccessibilityProviderResult.Failure = textFailure(
    "unsupported_on_surface",
    "Exact selection-aware ACTION_SET_TEXT is unavailable for this editor.",
    retryable = false,
)

private fun textFailure(
    code: String,
    message: String,
    retryable: Boolean,
    freshObservationRequired: Boolean = false,
) = AccessibilityProviderResult.Failure(code, message, retryable, freshObservationRequired)

internal data class InputBinding(val packageName: String, val fieldId: Int)

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
internal data class InputSession(
    val connection: InputMethod.AccessibilityInputConnection,
    val binding: InputBinding,
    val beforeText: String?,
    val selectionStart: Int,
    val selectionEnd: Int,
)

private data class NodeTextDispatch(val accepted: Boolean, val expected: String)

private const val ACCESSIBILITY_PROVIDER = "accessibility"
internal const val INPUT_METHOD_PROVIDER = "accessibility_input_method"
private const val POSTCONDITION_DELAY_MS = 120L
private const val SLOW_INPUT_DELAY_MS = 12L
private const val CANCELLED_INPUT_SETTLE_DELAY_MS = 40L
private const val ENTER_HOLD_MS = 45L
