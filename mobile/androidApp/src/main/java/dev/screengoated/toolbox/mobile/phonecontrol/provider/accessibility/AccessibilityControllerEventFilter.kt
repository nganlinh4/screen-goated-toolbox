package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

internal fun isKnownControllerOverlayEvent(
    eventWindowId: Int,
    eventPackage: String?,
    servicePackage: String,
    windows: List<AccessibilityWindowSnapshot>,
    knownControllerWindowIds: Set<Int> = emptySet(),
): Boolean {
    if (eventWindowId < 0) return false
    if (eventWindowId in knownControllerWindowIds) return true
    if (eventPackage != servicePackage) return false
    return windows.any { window ->
        window.id == eventWindowId &&
            window.packageName == servicePackage &&
            window.controllerOwned &&
            !isApplicationContentWindowType(window.type)
    }
}

internal fun shouldIgnoreControllerOverlayEvent(
    eventWindowId: Int,
    eventPackage: String?,
    servicePackage: String,
    windows: List<AccessibilityWindowSnapshot>,
    knownControllerWindowIds: Set<Int> = emptySet(),
    controllerTransitionActive: Boolean,
): Boolean {
    if (isKnownControllerOverlayEvent(
            eventWindowId,
            eventPackage,
            servicePackage,
            windows,
            knownControllerWindowIds,
        )
    ) {
        return true
    }
    return controllerTransitionActive &&
        eventWindowId < 0 &&
        eventPackage == servicePackage
}
