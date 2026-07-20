package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.accessibilityservice.InputMethod
import android.os.Build
import android.os.SystemClock
import android.view.KeyCharacterMap
import android.view.KeyEvent
import androidx.annotation.RequiresApi
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

internal data class AccessibilityKeyGroup(
    val keyCodes: List<Int>,
) {
    init {
        require(keyCodes.isNotEmpty())
    }
}

internal suspend fun performAccessibilityKeySequence(
    provider: PhoneControlAccessibilityProvider,
    target: AccessibilityTextTarget,
    groups: List<AccessibilityKeyGroup>,
    holdMs: Long,
): AccessibilityProviderResult<AccessibilityTextOutcome> {
    if (groups.isEmpty() || holdMs !in MIN_KEY_HOLD_MS..MAX_KEY_HOLD_MS) {
        return AccessibilityProviderResult.Failure(
            code = "invalid_action",
            message = "The key sequence or hold duration is invalid.",
            retryable = false,
        )
    }
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return unsupportedInputMethod()
    val lease = currentTextLease(provider, target) ?: return keyTargetFailure()
    provider.validateTargetMutation(
        lease,
        AccessibilityMutationKind.KEY_SEQUENCE,
        confirmed = false,
    )?.let { return it }
    return sendKeySequenceApi33(provider, lease, groups, holdMs)
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
private suspend fun sendKeySequenceApi33(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilityTargetLease,
    groups: List<AccessibilityKeyGroup>,
    holdMs: Long,
): AccessibilityProviderResult<AccessibilityTextOutcome> {
    val session = when (val prepared = prepareInputSession(
        provider,
        lease,
        AccessibilityMutationKind.KEY_SEQUENCE,
    )) {
        is AccessibilityProviderResult.Failure -> return prepared
        is AccessibilityProviderResult.Success -> prepared.value
    }
    var completed = 0
    var attempted = false
    try {
        for (group in groups) {
            currentCoroutineContext().ensureActive()
            when (val dispatched = sendAccessibilityKeyGroup(
                provider,
                lease,
                session.connection,
                session.binding,
                group,
                holdMs,
                AccessibilityMutationKind.KEY_SEQUENCE,
            )) {
                is AccessibilityProviderResult.Failure -> {
                    attempted = dispatched.effect.effectMayHaveOccurred != false
                    if (completed == 0) return dispatched
                    return dispatched.afterPartialKeyInput(completed)
                }
                is AccessibilityProviderResult.Success -> if (!dispatched.value) {
                    return AccessibilityProviderResult.Success(
                        partialKeyOutcome(
                            provider,
                            completed,
                            "The input connection closed or the editor changed before key dispatch.",
                        ),
                    )
                } else {
                    attempted = true
                }
            }
            completed += 1
            if (completed < groups.size) delay(KEY_GROUP_DELAY_MS)
        }
    } finally {
        if (completed > 0 || attempted) provider.invalidate("accessibility_input_method_keys")
    }
    return AccessibilityProviderResult.Success(
        AccessibilityTextOutcome(
            code = "ok",
            provider = INPUT_METHOD_PROVIDER,
            generation = provider.observationGeneration,
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            freshObservationRequired = true,
            completedKeyGroups = completed,
        ),
    )
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
internal suspend fun sendAccessibilityKeyGroup(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilityTargetLease,
    connection: InputMethod.AccessibilityInputConnection,
    binding: InputBinding,
    group: AccessibilityKeyGroup,
    holdMs: Long,
    kind: AccessibilityMutationKind,
): AccessibilityProviderResult<Boolean> {
    val ownedEffect = OwnedAccessibilityEffect.begin()
    val downTime = SystemClock.uptimeMillis()
    var metaState = 0
    val pressed = mutableListOf<Int>()
    try {
        val dispatched = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
            provider.validateTargetMutation(lease, kind, confirmed = false)
                ?.let { return@onServiceMain it }
            if (!matchesInputBinding(service, lease, binding)) {
                return@onServiceMain AccessibilityProviderResult.Success(false)
            }
            val accepted = ownedEffect.dispatchBoolean {
                for (keyCode in group.keyCodes) {
                    metaState = metaState or modifierMetaState(keyCode)
                    connection.sendKeyEvent(
                        keyEvent(downTime, KeyEvent.ACTION_DOWN, keyCode, metaState),
                    )
                    pressed += keyCode
                }
                true
            } ?: return@onServiceMain cancelledBeforeDispatch()
            if (!accepted) return@onServiceMain AccessibilityProviderResult.Success(false)
            AccessibilityProviderResult.Success(true)
        }
        when (dispatched) {
            is AccessibilityProviderResult.Failure -> return dispatched
            is AccessibilityProviderResult.Success -> if (!dispatched.value) return dispatched
        }
        delay(holdMs)
    } finally {
        try {
            withContext(NonCancellable) {
                val released = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) {
                    releaseAllKeys(connection, pressed, downTime, metaState)
                    AccessibilityProviderResult.Success(Unit)
                }
                if (released is AccessibilityProviderResult.Failure) {
                    runCatching { releaseAllKeys(connection, pressed, downTime, metaState) }
                }
                if (ownedEffect.wasAccepted) delay(KEY_SETTLE_DELAY_MS)
            }
        } finally {
            ownedEffect.close()
        }
    }
    return AccessibilityProviderResult.Success(true)
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
private fun releaseAllKeys(
    connection: InputMethod.AccessibilityInputConnection,
    pressed: List<Int>,
    downTime: Long,
    initialMetaState: Int,
) {
    var metaState = initialMetaState
    for (keyCode in pressed.asReversed()) {
        connection.sendKeyEvent(keyEvent(downTime, KeyEvent.ACTION_UP, keyCode, metaState))
        metaState = metaState and modifierMetaState(keyCode).inv()
    }
}

private fun keyEvent(downTime: Long, action: Int, keyCode: Int, metaState: Int): KeyEvent = KeyEvent(
    downTime,
    SystemClock.uptimeMillis(),
    action,
    keyCode,
    0,
    metaState,
    KeyCharacterMap.VIRTUAL_KEYBOARD,
    0,
    KeyEvent.FLAG_SOFT_KEYBOARD or KeyEvent.FLAG_KEEP_TOUCH_MODE,
)

private fun modifierMetaState(keyCode: Int): Int = when (keyCode) {
    KeyEvent.KEYCODE_CTRL_LEFT -> KeyEvent.META_CTRL_ON or KeyEvent.META_CTRL_LEFT_ON
    KeyEvent.KEYCODE_ALT_LEFT -> KeyEvent.META_ALT_ON or KeyEvent.META_ALT_LEFT_ON
    KeyEvent.KEYCODE_SHIFT_LEFT -> KeyEvent.META_SHIFT_ON or KeyEvent.META_SHIFT_LEFT_ON
    KeyEvent.KEYCODE_META_LEFT -> KeyEvent.META_META_ON or KeyEvent.META_META_LEFT_ON
    else -> 0
}

private fun partialKeyOutcome(
    provider: PhoneControlAccessibilityProvider,
    completed: Int,
    message: String,
) = AccessibilityTextOutcome(
    code = if (completed == 0) "stale_target" else "partial",
    provider = INPUT_METHOD_PROVIDER,
    generation = provider.observationGeneration,
    effect = if (completed == 0) EffectCertainty.PROVEN_NO_EFFECT else EffectCertainty.MAY_HAVE_OCCURRED,
    snapshotInvalidated = completed > 0,
    freshObservationRequired = true,
    completedKeyGroups = completed,
    message = message,
)

private fun AccessibilityProviderResult.Failure.afterPartialKeyInput(
    completed: Int,
) = copy(
    message = "Key input stopped after $completed completed groups. $message",
    freshObservationRequired = true,
    effect = EffectCertainty.MAY_HAVE_OCCURRED,
)

private fun keyTargetFailure() = AccessibilityProviderResult.Failure(
    code = "stale_target",
    message = "The focused editor target is stale or no longer focused.",
    retryable = true,
    freshObservationRequired = true,
)

private const val KEY_GROUP_DELAY_MS = 20L
private const val KEY_SETTLE_DELAY_MS = 40L
private const val MIN_KEY_HOLD_MS = 45L
private const val MAX_KEY_HOLD_MS = 10_000L
