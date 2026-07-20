package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.accessibilityservice.AccessibilityService
import android.graphics.Rect
import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSessionRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

internal enum class PlatformSettingsNavigationAction {
    OPEN_APP_ROW,
    SCROLL_FORWARD,
}

internal suspend fun performPlatformSettingsNavigation(
    provider: PhoneControlAccessibilityProvider,
    targetId: Int,
    action: PlatformSettingsNavigationAction,
    settingsPackage: String,
    appLabel: String,
): AccessibilityProviderResult<AccessibilityGestureOutcome> {
    if (!PlatformUserStepSessionRegistry.hasActiveSession()) {
        return platformNavigationFailure("platform_step_inactive")
    }
    val lease = provider.currentLease(targetId)
        ?: return platformNavigationFailure("stale_target", freshObservationRequired = true)
    val element = provider.currentElement(targetId)
        ?: return platformNavigationFailure("stale_target", freshObservationRequired = true)
    validatePlatformNavigationTarget(
        provider = provider,
        lease = lease,
        element = element,
        action = action,
        settingsPackage = settingsPackage,
        appLabel = appLabel,
    )?.let { return it }

    val ownedEffect = OwnedAccessibilityEffect.begin()
    try {
        val dispatched = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
            val currentElement = provider.currentElement(targetId)
                ?: return@onServiceMain platformNavigationFailure(
                    "stale_target",
                    freshObservationRequired = true,
                )
            validatePlatformNavigationTarget(
                provider = provider,
                lease = lease,
                element = currentElement,
                action = action,
                settingsPackage = settingsPackage,
                appLabel = appLabel,
            )?.let { return@onServiceMain it }
            val node = resolveAccessibilityNode(service, lease)
                ?.takeIf { candidate -> candidate.matches(lease) }
                ?: return@onServiceMain platformNavigationFailure(
                    "stale_target",
                    freshObservationRequired = true,
                )
            val accepted = when (action) {
                PlatformSettingsNavigationAction.OPEN_APP_ROW -> {
                    if (!node.hasExactLabel(appLabel)) {
                        return@onServiceMain platformNavigationFailure(
                            "target_mismatch",
                            freshObservationRequired = true,
                        )
                    }
                    val clickable = node.safeClickableAncestor(lease.surfaceLease.bounds)
                        ?: return@onServiceMain platformNavigationFailure("unsafe_navigation_target")
                    ownedEffect.dispatchBoolean {
                        clickable.performAction(AccessibilityNodeInfo.ACTION_CLICK)
                    }
                }
                PlatformSettingsNavigationAction.SCROLL_FORWARD -> {
                    if (!node.isScrollable ||
                        !node.supportsAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)
                    ) {
                        return@onServiceMain platformNavigationFailure(
                            "target_mismatch",
                            freshObservationRequired = true,
                        )
                    }
                    ownedEffect.dispatchBoolean {
                        node.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)
                    }
                }
            } ?: return@onServiceMain cancelledBeforeDispatch()
            AccessibilityProviderResult.Success(accepted)
        }
        val accepted = when (dispatched) {
            is AccessibilityProviderResult.Failure -> return dispatched
            is AccessibilityProviderResult.Success -> dispatched.value
        }
        if (!accepted) {
            return AccessibilityProviderResult.Success(
                AccessibilityGestureOutcome(
                    code = "navigation_rejected",
                    generation = provider.observationGeneration,
                    effect = EffectCertainty.PROVEN_NO_EFFECT,
                    snapshotInvalidated = false,
                ),
            )
        }
        provider.invalidate("platform_settings_navigation:${action.name.lowercase()}")
        withContext(NonCancellable) { delay(NAVIGATION_SETTLE_MS) }
        return AccessibilityProviderResult.Success(
            AccessibilityGestureOutcome(
                code = "ok",
                generation = provider.observationGeneration,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
            ),
        )
    } finally {
        ownedEffect.close()
    }
}

internal suspend fun performPlatformSettingsBackNavigation(
    provider: PhoneControlAccessibilityProvider,
    observation: AccessibilityObservation,
    settingsPackage: String,
): AccessibilityProviderResult<AccessibilityGestureOutcome> {
    if (!PlatformUserStepSessionRegistry.hasActiveSession()) {
        return platformNavigationFailure("platform_step_inactive")
    }
    val window = observation.activeSettingsWindow(settingsPackage)
        ?: return platformNavigationFailure("settings_surface_changed")
    if (observation.generation != provider.observationGeneration ||
        window.targetAuthority != AccessibilityTargetAuthority.OS_OWNED_USER_STEP
    ) {
        return platformNavigationFailure("stale_target", freshObservationRequired = true)
    }
    val ownedEffect = OwnedAccessibilityEffect.begin()
    try {
        val dispatched = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
            val current = provider.currentObservation()
            if (current?.generation != observation.generation ||
                current.activeSettingsWindow(settingsPackage) != window
            ) {
                return@onServiceMain platformNavigationFailure(
                    "stale_target",
                    freshObservationRequired = true,
                )
            }
            val accepted = ownedEffect.dispatchBoolean {
                service.performGlobalAction(AccessibilityService.GLOBAL_ACTION_BACK)
            } ?: return@onServiceMain cancelledBeforeDispatch()
            AccessibilityProviderResult.Success(accepted)
        }
        val accepted = when (dispatched) {
            is AccessibilityProviderResult.Failure -> return dispatched
            is AccessibilityProviderResult.Success -> dispatched.value
        }
        if (!accepted) {
            return AccessibilityProviderResult.Success(
                AccessibilityGestureOutcome(
                    code = "navigation_rejected",
                    generation = provider.observationGeneration,
                    effect = EffectCertainty.PROVEN_NO_EFFECT,
                    snapshotInvalidated = false,
                ),
            )
        }
        provider.invalidate("platform_settings_navigation:back")
        withContext(NonCancellable) { delay(NAVIGATION_SETTLE_MS) }
        return AccessibilityProviderResult.Success(
            AccessibilityGestureOutcome(
                code = "ok",
                generation = provider.observationGeneration,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
            ),
        )
    } finally {
        ownedEffect.close()
    }
}

private fun AccessibilityObservation.activeSettingsWindow(
    settingsPackage: String,
): AccessibilityWindowSnapshot? = windows
    .asSequence()
    .filter { window ->
        !window.controllerOwned && window.packageName == settingsPackage &&
            (window.active || window.focused)
    }
    .maxByOrNull(AccessibilityWindowSnapshot::layer)

private fun validatePlatformNavigationTarget(
    provider: PhoneControlAccessibilityProvider,
    lease: AccessibilityTargetLease,
    element: AccessibilityElement,
    action: PlatformSettingsNavigationAction,
    settingsPackage: String,
    appLabel: String,
): AccessibilityProviderResult.Failure? {
    if (provider.currentCaptureGeneration() != lease.identity.snapshotGeneration ||
        provider.observationGeneration != lease.identity.snapshotGeneration
    ) {
        return platformNavigationFailure("stale_target", freshObservationRequired = true)
    }
    if (element.controllerOwned || !element.visible || !element.enabled || element.isProtected ||
        element.packageName != settingsPackage || lease.fingerprint.packageName != settingsPackage ||
        element.targetAuthority != AccessibilityTargetAuthority.OS_OWNED_USER_STEP
    ) {
        return platformNavigationFailure("unsafe_navigation_target")
    }
    return when (action) {
        PlatformSettingsNavigationAction.OPEN_APP_ROW ->
            platformNavigationFailure("target_mismatch").takeIf { element.label != appLabel }
        PlatformSettingsNavigationAction.SCROLL_FORWARD ->
            platformNavigationFailure("target_mismatch").takeUnless {
                "scroll_forward" in element.actions && element.role == "scroll_container"
            }
    }
}

private fun AccessibilityNodeInfo.safeClickableAncestor(
    surfaceBounds: TargetBounds,
): AccessibilityNodeInfo? {
    var candidate: AccessibilityNodeInfo? = this
    repeat(MAX_ANCESTOR_DEPTH) {
        val current = candidate ?: return null
        if (current.isCheckable) return null
        if (current.isClickable && current.supportsAction(AccessibilityNodeInfo.ACTION_CLICK)) {
            val bounds = Rect().also(current::getBoundsInScreen)
            if (!surfaceBounds.contains(bounds) || current.hasCheckableDescendant()) return null
            return current
        }
        candidate = current.parent
    }
    return null
}

private fun AccessibilityNodeInfo.hasCheckableDescendant(): Boolean {
    val pending = ArrayDeque<AccessibilityNodeInfo>()
    pending.add(this)
    var visited = 0
    while (pending.isNotEmpty() && visited++ < MAX_DESCENDANT_NODES) {
        val node = pending.removeFirst()
        if (node.isCheckable) return true
        repeat(node.childCount) { index -> node.getChild(index)?.let(pending::addLast) }
    }
    return pending.isNotEmpty()
}

private fun AccessibilityNodeInfo.hasExactLabel(label: String): Boolean =
    text?.toString() == label || contentDescription?.toString() == label

private fun TargetBounds.contains(rect: Rect): Boolean =
    rect.left >= left && rect.top >= top && rect.right <= right && rect.bottom <= bottom

private fun platformNavigationFailure(
    code: String,
    freshObservationRequired: Boolean = false,
) = AccessibilityProviderResult.Failure(
    code = code,
    message = "The Android settings navigation step could not proceed safely.",
    retryable = code == "stale_target",
    freshObservationRequired = freshObservationRequired,
)

private const val MAX_ANCESTOR_DEPTH = 8
private const val MAX_DESCENDANT_NODES = 128
private const val NAVIGATION_SETTLE_MS = 180L
