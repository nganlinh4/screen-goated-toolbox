package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.graphics.Path
import android.os.Build
import android.os.Bundle
import android.view.Display
import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.ceil
import kotlin.math.floor

internal suspend fun performAccessibilityAction(
    provider: PhoneControlAccessibilityProvider,
    targetId: Int,
    verb: AccessibilityActionVerb,
    value: String?,
    confirmed: Boolean,
): AccessibilityProviderResult<AccessibilityActionOutcome> {
    val ownedEffect = OwnedAccessibilityEffect.begin()
    var platformSettled = false
    try {
    val lease = provider.currentLease(targetId) ?: return stale(provider.observationGeneration)
    if (provider.currentCaptureGeneration() != lease.identity.snapshotGeneration ||
        provider.observationGeneration != lease.identity.snapshotGeneration
    ) {
        return stale(provider.observationGeneration)
    }
    provider.validateTargetMutation(lease, verb.mutationKind(), confirmed)?.let { return it }

    val dispatch = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
        provider.validateTargetMutation(lease, verb.mutationKind(), confirmed)?.let {
            return@onServiceMain it
        }
        val node = resolveAccessibilityNode(service, lease)
            ?: return@onServiceMain stale(provider.observationGeneration)
        if (!node.matches(lease) || !node.isVisibleToUser || !node.isEnabled) {
            return@onServiceMain stale(provider.observationGeneration)
        }
        val structuralFailure = validateVerb(node, verb, value)
        if (structuralFailure != null) {
            return@onServiceMain AccessibilityProviderResult.Failure(
                code = "invalid_action",
                message = structuralFailure,
                retryable = false,
            )
        }
        val before = ActionPostcondition(
            text = node.text?.toString(),
            checked = node.checkedBoolean().takeIf { node.isCheckable },
            selected = node.isSelected,
        )
        val dispatched = ownedEffect.dispatchBoolean {
            when (verb) {
                AccessibilityActionVerb.CLICK,
                AccessibilityActionVerb.ACTIVATE,
                AccessibilityActionVerb.SUBMIT,
                AccessibilityActionVerb.TOGGLE,
                -> node.performAction(AccessibilityNodeInfo.ACTION_CLICK)
                AccessibilityActionVerb.FILL -> node.performAction(
                    AccessibilityNodeInfo.ACTION_SET_TEXT,
                    Bundle().apply {
                        putCharSequence(
                            AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                            requireNotNull(value),
                        )
                    },
                )
                AccessibilityActionVerb.SELECT ->
                    node.performAction(AccessibilityNodeInfo.ACTION_SELECT)
            }
        } ?: return@onServiceMain cancelledBeforeDispatch()
        AccessibilityProviderResult.Success(ActionDispatch(dispatched, before))
    }
    val dispatched = when (dispatch) {
        is AccessibilityProviderResult.Failure -> return dispatch
        is AccessibilityProviderResult.Success -> dispatch.value
    }
    if (!dispatched.dispatched) {
        return AccessibilityProviderResult.Success(
            AccessibilityActionOutcome(
                code = "action_rejected",
                generation = provider.observationGeneration,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                freshObservationRequired = false,
            ),
        )
    }

    provider.invalidate("accessibility_action:$verb")
    val verified = withContext(NonCancellable) {
        delay(POSTCONDITION_DELAY_MS)
        provider.onServiceMain { service ->
            val current = resolveAccessibilityNode(service, lease)
            AccessibilityProviderResult.Success(
                current?.let { node ->
                    verifyPostcondition(node, verb, value, dispatched.before)
                } == true,
            )
        }
    }
    platformSettled = true
    val effect = if ((verified as? AccessibilityProviderResult.Success)?.value == true) {
        EffectCertainty.VERIFIED
    } else {
        EffectCertainty.MAY_HAVE_OCCURRED
    }
    return AccessibilityProviderResult.Success(
        AccessibilityActionOutcome(
            code = "ok",
            generation = provider.observationGeneration,
            effect = effect,
            snapshotInvalidated = true,
            freshObservationRequired = true,
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

internal suspend fun dispatchAccessibilityClick(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilitySurfaceLease,
    x: Float,
    y: Float,
    confirmed: Boolean,
    expectedVisualRevision: Long?,
): AccessibilityProviderResult<AccessibilityGestureOutcome> {
    if (!x.isFinite() || !y.isFinite() || x < 0f || y < 0f) {
        return invalidGesture(provider.observationGeneration, "Click coordinates are invalid.")
    }
    if (!lease.bounds.contains(x, y)) {
        return invalidGesture(provider.observationGeneration, "Click coordinates are outside the leased surface.")
    }
    return dispatchGesture(
        provider,
        lease,
        AccessibilityMutationKind.POINTER_ACTIVATE,
        confirmed,
        Path().apply { moveTo(x, y) },
        CLICK_DURATION_MS,
        gestureBounds(x, y, x, y),
        expectedVisualRevision,
    )
}

internal suspend fun dispatchAccessibilitySwipe(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilitySurfaceLease,
    fromX: Float,
    fromY: Float,
    toX: Float,
    toY: Float,
    durationMs: Long,
    kind: AccessibilityMutationKind,
    confirmed: Boolean,
    expectedVisualRevision: Long?,
): AccessibilityProviderResult<AccessibilityGestureOutcome> {
    val points = listOf(fromX, fromY, toX, toY)
    if (points.any { !it.isFinite() || it < 0f } || durationMs !in 50L..10_000L) {
        return invalidGesture(provider.observationGeneration, "Swipe geometry or duration is invalid.")
    }
    if (!lease.bounds.contains(fromX, fromY) || !lease.bounds.contains(toX, toY)) {
        return invalidGesture(provider.observationGeneration, "Swipe endpoints are outside the leased surface.")
    }
    val path = Path().apply {
        moveTo(fromX, fromY)
        lineTo(toX, toY)
    }
    return dispatchGesture(
        provider,
        lease,
        kind,
        confirmed,
        path,
        durationMs,
        gestureBounds(fromX, fromY, toX, toY),
        expectedVisualRevision,
    )
}

internal suspend fun performAccessibilityGlobalAction(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilitySurfaceLease,
    action: Int,
): AccessibilityProviderResult<AccessibilityGestureOutcome> {
    val ownedEffect = OwnedAccessibilityEffect.begin()
    var platformSettled = false
    try {
    if (action !in SUPPORTED_GLOBAL_ACTIONS) {
        return invalidGesture(provider.observationGeneration, "Global action is not in the reviewed set.")
    }
    val result = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
        provider.validateSurfaceMutation(
            lease,
            AccessibilityMutationKind.NAVIGATION_GESTURE,
            confirmed = false,
            affectedBounds = null,
        )?.let { return@onServiceMain it }
        val accepted = ownedEffect.dispatchBoolean { service.performGlobalAction(action) }
            ?: return@onServiceMain cancelledBeforeDispatch()
        AccessibilityProviderResult.Success(accepted)
    }
    val dispatched = (result as? AccessibilityProviderResult.Success)?.value
        ?: return result as AccessibilityProviderResult.Failure
    if (!dispatched) {
        return AccessibilityProviderResult.Success(
            AccessibilityGestureOutcome(
                code = "action_rejected",
                generation = provider.observationGeneration,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
            ),
        )
    }
    provider.invalidate("global_action:$action")
    withContext(NonCancellable) { delay(POSTCONDITION_DELAY_MS) }
    platformSettled = true
    return AccessibilityProviderResult.Success(
        AccessibilityGestureOutcome(
            code = "ok",
            generation = provider.observationGeneration,
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
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

private suspend fun dispatchGesture(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilitySurfaceLease,
    kind: AccessibilityMutationKind,
    confirmed: Boolean,
    path: Path,
    durationMs: Long,
    affectedBounds: TargetBounds,
    expectedVisualRevision: Long?,
): AccessibilityProviderResult<AccessibilityGestureOutcome> {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R && lease.displayId != Display.DEFAULT_DISPLAY) {
        return AccessibilityProviderResult.Failure(
            code = "unsupported_display",
            message = "Gesture dispatch to this display is unavailable on this Android version.",
            retryable = false,
        )
    }
    val ownedEffect = OwnedAccessibilityEffect.begin()
    val completion = CompletableDeferred<GestureDispatchCompletion>()
    val platformAccepted = AtomicBoolean(false)
    try {
    val started = provider.onServiceMain { service ->
        provider.validateVisualRevision(expectedVisualRevision)?.let {
            return@onServiceMain it
        }
        provider.validateSurfaceMutation(lease, kind, confirmed, affectedBounds)?.let {
            return@onServiceMain it
        }
        val builder = GestureDescription.Builder()
            .addStroke(GestureDescription.StrokeDescription(path, 0L, durationMs))
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) builder.setDisplayId(lease.displayId)
        val accepted = ownedEffect.dispatchBoolean {
            service.dispatchGesture(
                builder.build(),
                object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(gestureDescription: GestureDescription?) {
                        completion.complete(GestureDispatchCompletion.COMPLETED)
                        ownedEffect.close()
                    }

                    override fun onCancelled(gestureDescription: GestureDescription?) {
                        completion.complete(GestureDispatchCompletion.CANCELLED_AFTER_ACCEPTANCE)
                        ownedEffect.close()
                    }
                },
                null,
            )
        } ?: return@onServiceMain cancelledBeforeDispatch()
        if (accepted) {
            platformAccepted.set(true)
            provider.invalidate("gesture_dispatch_accepted")
        }
        AccessibilityProviderResult.Success(accepted)
    }
    val accepted = when (started) {
        is AccessibilityProviderResult.Failure -> return started
        is AccessibilityProviderResult.Success -> started.value
    }
    if (!accepted) {
        return AccessibilityProviderResult.Success(
            gestureDispatchOutcome(
                GestureDispatchCompletion.REJECTED_BEFORE_DISPATCH,
                provider.observationGeneration,
            ),
        )
    }
    val finalState = withContext(NonCancellable) { completion.await() }
    return AccessibilityProviderResult.Success(
        gestureDispatchOutcome(finalState, provider.observationGeneration),
    )
    } finally {
        if (!platformAccepted.get()) ownedEffect.close()
    }
}

internal enum class GestureDispatchCompletion {
    REJECTED_BEFORE_DISPATCH,
    COMPLETED,
    CANCELLED_AFTER_ACCEPTANCE,
}

internal fun gestureDispatchOutcome(
    completion: GestureDispatchCompletion,
    generation: Long,
): AccessibilityGestureOutcome = when (completion) {
    GestureDispatchCompletion.REJECTED_BEFORE_DISPATCH -> AccessibilityGestureOutcome(
        code = "gesture_rejected",
        generation = generation,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
    )
    GestureDispatchCompletion.COMPLETED -> AccessibilityGestureOutcome(
        code = "ok",
        generation = generation,
        effect = EffectCertainty.MAY_HAVE_OCCURRED,
        snapshotInvalidated = true,
    )
    GestureDispatchCompletion.CANCELLED_AFTER_ACCEPTANCE -> AccessibilityGestureOutcome(
        code = "gesture_cancelled",
        generation = generation,
        effect = EffectCertainty.MAY_HAVE_OCCURRED,
        snapshotInvalidated = true,
    )
}

private fun TargetBounds.contains(x: Float, y: Float): Boolean =
    x >= left && x <= right && y >= top && y <= bottom

private fun gestureBounds(fromX: Float, fromY: Float, toX: Float, toY: Float): TargetBounds {
    val left = floor(minOf(fromX, toX).toDouble()).toInt()
    val top = floor(minOf(fromY, toY).toDouble()).toInt()
    val right = ceil(maxOf(fromX, toX).toDouble()).toInt().coerceAtLeast(left + 1)
    val bottom = ceil(maxOf(fromY, toY).toDouble()).toInt().coerceAtLeast(top + 1)
    return TargetBounds(left, top, right, bottom)
}

internal fun resolveAccessibilityNode(
    service: dev.screengoated.toolbox.mobile.service.SgtAccessibilityService,
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

private fun validateVerb(
    node: AccessibilityNodeInfo,
    verb: AccessibilityActionVerb,
    value: String?,
): String? = when (verb) {
    AccessibilityActionVerb.FILL -> when {
        value == null -> "fill requires a value"
        !node.isEditable && !node.supportsAction(AccessibilityNodeInfo.ACTION_SET_TEXT) ->
            "target does not support text editing"
        else -> null
    }
    AccessibilityActionVerb.TOGGLE -> if (node.isCheckable) null else "target is not checkable"
    AccessibilityActionVerb.SELECT -> if (
        node.supportsAction(AccessibilityNodeInfo.ACTION_SELECT)
    ) null else "target does not support selection"
    AccessibilityActionVerb.CLICK,
    AccessibilityActionVerb.ACTIVATE,
    AccessibilityActionVerb.SUBMIT,
    -> if (node.isClickable || node.supportsAction(AccessibilityNodeInfo.ACTION_CLICK)) {
        null
    } else {
        "target does not support activation"
    }
}

private fun verifyPostcondition(
    node: AccessibilityNodeInfo,
    verb: AccessibilityActionVerb,
    value: String?,
    before: ActionPostcondition,
): Boolean = when (verb) {
    AccessibilityActionVerb.FILL -> node.text?.toString() == value
    AccessibilityActionVerb.TOGGLE ->
        before.checked != null && node.checkedBoolean() != before.checked
    AccessibilityActionVerb.SELECT -> node.isSelected && !before.selected
    AccessibilityActionVerb.CLICK,
    AccessibilityActionVerb.ACTIVATE,
    AccessibilityActionVerb.SUBMIT,
    -> false
}

private fun stale(generation: Long): AccessibilityProviderResult.Failure =
    AccessibilityProviderResult.Failure(
        code = "stale_target",
        message = "The target does not belong to the current observation.",
        retryable = true,
        freshObservationRequired = true,
    )

private fun invalidGesture(
    generation: Long,
    message: String,
): AccessibilityProviderResult<AccessibilityGestureOutcome> = AccessibilityProviderResult.Failure(
    code = "invalid_action",
    message = message,
    retryable = false,
)

private data class ActionPostcondition(
    val text: String?,
    val checked: Boolean?,
    val selected: Boolean,
)

private data class ActionDispatch(
    val dispatched: Boolean,
    val before: ActionPostcondition,
)


private val SUPPORTED_GLOBAL_ACTIONS = setOf(
    AccessibilityService.GLOBAL_ACTION_BACK,
    AccessibilityService.GLOBAL_ACTION_HOME,
    AccessibilityService.GLOBAL_ACTION_RECENTS,
    AccessibilityService.GLOBAL_ACTION_NOTIFICATIONS,
    AccessibilityService.GLOBAL_ACTION_QUICK_SETTINGS,
)

private const val CLICK_DURATION_MS = 80L
private const val POSTCONDITION_DELAY_MS = 120L
