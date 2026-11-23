package dev.screengoated.toolbox.mobile.service

import android.os.Build
import android.view.accessibility.AccessibilityEvent
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider

internal fun distributionAccessibilityServiceConnected(service: SgtAccessibilityService) {
    phoneControlAccessibilityDemand.attach(service) { phoneControlActive ->
        service.applyAccessibilityEventProfile(accessibilityEventProfile(phoneControlActive, Build.VERSION.SDK_INT))
        // Events were filtered while narrow, so no earlier capture can be trusted.
        if (phoneControlActive) PhoneControlAccessibilityProvider.invalidate("event_profile_widened")
    }
    PhoneControlAccessibilityProvider.attach(service)
}

internal fun distributionAccessibilityEvent(
    service: SgtAccessibilityService,
    event: AccessibilityEvent?,
) {
    PhoneControlAccessibilityProvider.onAccessibilityEvent(service, event)
}

internal fun distributionAccessibilityServiceDestroyed(service: SgtAccessibilityService) {
    phoneControlAccessibilityDemand.detach(service)
    PhoneControlAccessibilityProvider.detach(service)
}

private fun SgtAccessibilityService.applyAccessibilityEventProfile(profile: AccessibilityEventProfile) {
    val info = serviceInfo ?: return
    info.eventTypes = profile.eventTypes
    info.notificationTimeout = profile.notificationTimeoutMs
    info.flags = profile.flags
    serviceInfo = info
}
