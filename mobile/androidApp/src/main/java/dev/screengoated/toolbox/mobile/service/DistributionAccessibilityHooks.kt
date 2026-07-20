package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityServiceInfo
import android.os.Build
import android.view.accessibility.AccessibilityEvent
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider

internal fun distributionAccessibilityServiceConnected(service: SgtAccessibilityService) {
    service.serviceInfo = service.serviceInfo.apply {
        eventTypes = AccessibilityEvent.TYPES_ALL_MASK
        notificationTimeout = 50
        flags = flags or
            AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS or
            AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS or
            AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            flags = flags or AccessibilityServiceInfo.FLAG_INPUT_METHOD_EDITOR
        }
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
    PhoneControlAccessibilityProvider.detach(service)
}
