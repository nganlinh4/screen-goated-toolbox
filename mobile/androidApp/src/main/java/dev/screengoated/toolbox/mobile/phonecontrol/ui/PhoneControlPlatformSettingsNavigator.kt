package dev.screengoated.toolbox.mobile.phonecontrol.ui

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PlatformSettingsNavigationAction
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.performPlatformSettingsBackNavigation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.performPlatformSettingsNavigation
import kotlinx.coroutines.delay

internal sealed interface PlatformSettingsNavigationDecision {
    data class OpenRow(val targetId: Int) : PlatformSettingsNavigationDecision
    data class ScrollForward(val targetId: Int) : PlatformSettingsNavigationDecision
    data object WaitForSurface : PlatformSettingsNavigationDecision
    data object AmbiguousTarget : PlatformSettingsNavigationDecision
}

internal enum class PlatformSettingsNavigationResult(val wireName: String) {
    OPENED_APP_ROW("opened_app_row"),
    PERMISSION_ALREADY_READY("permission_already_ready"),
    RETURNED_AFTER_PERMISSION("returned_after_permission"),
    AMBIGUOUS_TARGET("ambiguous_target"),
    TARGET_NOT_FOUND("target_not_found"),
    DISPATCH_FAILED("dispatch_failed"),
}

internal object PhoneControlPlatformSettingsNavigator {
    suspend fun openAppRow(
        settingsPackage: String,
        appLabel: String,
        permissionReady: () -> Boolean,
    ): PlatformSettingsNavigationResult {
        var scrolls = 0
        var dispatchFailures = 0
        var appRowOpened = false
        repeat(MAX_NAVIGATION_POLLS) {
            if (permissionReady()) {
                PhoneControlLog.i(TAG, "settings_navigation phase=grant_observed")
                return returnFromSettings(settingsPackage)
            }
            if (appRowOpened) {
                delay(POLL_MS)
                return@repeat
            }
            when (val observed = PhoneControlAccessibilityProvider.observe(MAX_ELEMENTS)) {
                is AccessibilityProviderResult.Failure -> delay(POLL_MS)
                is AccessibilityProviderResult.Success -> when (
                    val decision = choosePlatformSettingsNavigation(
                        observed.value,
                        settingsPackage,
                        appLabel,
                    )
                ) {
                    PlatformSettingsNavigationDecision.AmbiguousTarget ->
                        return PlatformSettingsNavigationResult.AMBIGUOUS_TARGET
                    PlatformSettingsNavigationDecision.WaitForSurface -> delay(POLL_MS)
                    is PlatformSettingsNavigationDecision.OpenRow -> {
                        val outcome = performPlatformSettingsNavigation(
                            provider = PhoneControlAccessibilityProvider,
                            targetId = decision.targetId,
                            action = PlatformSettingsNavigationAction.OPEN_APP_ROW,
                            settingsPackage = settingsPackage,
                            appLabel = appLabel,
                        )
                        if (outcome is AccessibilityProviderResult.Success &&
                            outcome.value.code == "ok"
                        ) {
                            appRowOpened = true
                            PhoneControlLog.i(TAG, "settings_navigation phase=app_row_opened")
                            delay(POLL_MS)
                            return@repeat
                        }
                        if (++dispatchFailures >= MAX_DISPATCH_FAILURES) {
                            return PlatformSettingsNavigationResult.DISPATCH_FAILED
                        }
                        delay(POLL_MS)
                    }
                    is PlatformSettingsNavigationDecision.ScrollForward -> {
                        if (scrolls++ >= MAX_SCROLLS) {
                            return PlatformSettingsNavigationResult.TARGET_NOT_FOUND
                        }
                        val outcome = performPlatformSettingsNavigation(
                            provider = PhoneControlAccessibilityProvider,
                            targetId = decision.targetId,
                            action = PlatformSettingsNavigationAction.SCROLL_FORWARD,
                            settingsPackage = settingsPackage,
                            appLabel = appLabel,
                        )
                        if (outcome !is AccessibilityProviderResult.Success ||
                            outcome.value.code != "ok"
                        ) {
                            if (++dispatchFailures >= MAX_DISPATCH_FAILURES) {
                                return PlatformSettingsNavigationResult.DISPATCH_FAILED
                            }
                        }
                        delay(SCROLL_SETTLE_MS)
                    }
                }
            }
        }
        return if (appRowOpened) {
            PlatformSettingsNavigationResult.OPENED_APP_ROW
        } else {
            PlatformSettingsNavigationResult.TARGET_NOT_FOUND
        }
    }

    private suspend fun returnFromSettings(
        settingsPackage: String,
    ): PlatformSettingsNavigationResult {
        repeat(MAX_SETTINGS_BACKS) {
            val observed = PhoneControlAccessibilityProvider.observe(MAX_ELEMENTS)
            if (observed !is AccessibilityProviderResult.Success) {
                delay(POLL_MS)
                return@repeat
            }
            if (!observed.value.hasActivePackage(settingsPackage)) {
                PhoneControlLog.i(TAG, "settings_navigation phase=returned")
                return PlatformSettingsNavigationResult.RETURNED_AFTER_PERMISSION
            }
            val outcome = performPlatformSettingsBackNavigation(
                provider = PhoneControlAccessibilityProvider,
                observation = observed.value,
                settingsPackage = settingsPackage,
            )
            if (outcome !is AccessibilityProviderResult.Success || outcome.value.code != "ok") {
                return PlatformSettingsNavigationResult.DISPATCH_FAILED
            }
            PhoneControlLog.i(TAG, "settings_navigation phase=back_dispatched index=${it + 1}")
            delay(BACK_SETTLE_MS)
        }
        return PlatformSettingsNavigationResult.DISPATCH_FAILED
    }
}

private fun AccessibilityObservation.hasActivePackage(packageName: String): Boolean =
    windows.any { window ->
        !window.controllerOwned && window.packageName == packageName &&
            (window.active || window.focused)
    }

internal fun choosePlatformSettingsNavigation(
    observation: AccessibilityObservation,
    settingsPackage: String,
    appLabel: String,
): PlatformSettingsNavigationDecision {
    val window = observation.windows
        .asSequence()
        .filter { candidate ->
            !candidate.controllerOwned && candidate.packageName == settingsPackage &&
                (candidate.active || candidate.focused)
        }
        .maxByOrNull { candidate -> candidate.layer }
        ?: return PlatformSettingsNavigationDecision.WaitForSurface
    val elements = observation.elements.filter { element ->
        element.visible && element.enabled && !element.controllerOwned && !element.isProtected &&
            element.packageName == settingsPackage &&
            element.target.displayId == window.displayId &&
            element.target.windowId == window.id.toLong()
    }
    val matches = elements.filter { element -> element.label == appLabel }
    if (matches.size > 1) return PlatformSettingsNavigationDecision.AmbiguousTarget
    matches.singleOrNull()?.let { match ->
        return PlatformSettingsNavigationDecision.OpenRow(match.id)
    }
    return elements
        .asSequence()
        .filter { element ->
            element.role == "scroll_container" && "scroll_forward" in element.actions
        }
        .maxByOrNull { element -> element.bounds.area() }
        ?.let { element -> PlatformSettingsNavigationDecision.ScrollForward(element.id) }
        ?: PlatformSettingsNavigationDecision.WaitForSurface
}

private fun dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds.area(): Long =
    (right - left).toLong() * (bottom - top).toLong()

private const val MAX_ELEMENTS = 600
private const val MAX_NAVIGATION_POLLS = 480
private const val MAX_SCROLLS = 10
private const val MAX_SETTINGS_BACKS = 4
private const val MAX_DISPATCH_FAILURES = 3
private const val POLL_MS = 250L
private const val SCROLL_SETTLE_MS = 450L
private const val BACK_SETTLE_MS = 350L
private const val TAG = "SGTPhoneControlSettings"
