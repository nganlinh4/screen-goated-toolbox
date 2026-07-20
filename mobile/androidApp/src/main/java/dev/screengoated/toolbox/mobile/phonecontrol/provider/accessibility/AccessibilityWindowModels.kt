package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.os.Build
import android.view.Display
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds

internal data class AccessibilityDisplayWindows<T>(
    val displayId: Int,
    val windows: List<T>,
) {
    init {
        require(displayId >= 0) { "display id must be non-negative" }
    }
}

internal data class AccessibilityWindowOnDisplay<T>(
    val displayId: Int,
    val window: T,
)

internal data class CapturedAccessibilityWindow<T>(
    val displayId: Int,
    val id: Int,
    val layer: Int,
    val type: String,
    val title: String?,
    val packageName: String?,
    val active: Boolean,
    val focused: Boolean,
    val bounds: TargetBounds,
    val accessibilityOverlay: Boolean,
    val pictureInPicture: Boolean,
    val root: T?,
    val targetAuthority: AccessibilityTargetAuthority = AccessibilityTargetAuthority.ROUTINE,
)

internal data class ActiveAccessibilityRoot<T>(
    val displayId: Int,
    val id: Int,
    val packageName: String?,
    val bounds: TargetBounds,
    val root: T,
) {
    init {
        require(displayId >= 0) { "display id must be non-negative" }
    }
}

internal data class AccessibilityDisplayExtent(
    val displayId: Int,
    val bounds: TargetBounds,
) {
    init {
        require(displayId >= 0) { "display id must be non-negative" }
    }
}

internal fun <T> selectAccessibilityWindows(
    apiLevel: Int,
    defaultWindows: () -> List<T>,
    allDisplayWindows: () -> List<AccessibilityDisplayWindows<T>>,
): List<AccessibilityWindowOnDisplay<T>> =
    if (apiLevel >= Build.VERSION_CODES.R) {
        allDisplayWindows().flatMap { group ->
            group.windows.map { window -> AccessibilityWindowOnDisplay(group.displayId, window) }
        }
    } else {
        defaultWindows().map { window ->
            AccessibilityWindowOnDisplay(Display.DEFAULT_DISPLAY, window)
        }
    }

internal fun <T> snapshotAccessibilityWindows(
    windows: List<CapturedAccessibilityWindow<T>>,
    servicePackage: String,
): List<AccessibilityWindowSnapshot> = windows.map { window ->
    AccessibilityWindowSnapshot(
        id = window.id,
        displayId = window.displayId,
        layer = window.layer,
        type = window.type,
        title = window.title,
        packageName = window.packageName,
        active = window.active,
        focused = window.focused,
        bounds = window.bounds,
        contentAccessible = window.root != null,
        controllerOwned = isControllerOwnedWindow(
            accessibilityOverlay = window.accessibilityOverlay,
            packageName = window.packageName,
            type = window.type,
            servicePackage = servicePackage,
        ),
        pictureInPicture = window.pictureInPicture,
        targetAuthority = window.targetAuthority,
    )
}

internal fun <T> supplementMissingActiveRoot(
    windows: List<CapturedAccessibilityWindow<T>>,
    activeRoot: ActiveAccessibilityRoot<T>?,
): List<CapturedAccessibilityWindow<T>> {
    if (activeRoot == null || windows.any { window ->
            window.displayId == activeRoot.displayId && window.id == activeRoot.id
        }
    ) {
        return windows
    }
    val highestLayer = windows.maxOfOrNull(CapturedAccessibilityWindow<T>::layer) ?: 0
    val syntheticLayer = if (highestLayer == Int.MAX_VALUE) highestLayer else highestLayer + 1
    return windows + CapturedAccessibilityWindow(
        displayId = activeRoot.displayId,
        id = activeRoot.id,
        layer = syntheticLayer,
        type = ACTIVE_CONTENT_WINDOW_TYPE,
        title = null,
        packageName = activeRoot.packageName,
        active = true,
        focused = false,
        bounds = activeRoot.bounds,
        accessibilityOverlay = false,
        pictureInPicture = false,
        root = activeRoot.root,
    )
}

internal fun resolveActiveRootDisplay(
    rootBounds: TargetBounds,
    displays: List<AccessibilityDisplayExtent>,
): Int? {
    val exact = displays.filter { display -> display.bounds == rootBounds }
    if (exact.size == 1) return exact.single().displayId
    val containing = displays.filter { display -> display.bounds.contains(rootBounds) }
    return containing.singleOrNull()?.displayId
}

internal fun <T> appendMissingAccessibilityWindow(
    windows: List<AccessibilityWindowOnDisplay<T>>,
    candidate: AccessibilityWindowOnDisplay<T>?,
    windowId: (T) -> Int,
): List<AccessibilityWindowOnDisplay<T>> {
    if (candidate == null || windows.any { window ->
            window.displayId == candidate.displayId && windowId(window.window) == windowId(candidate.window)
        }
    ) {
        return windows
    }
    return windows + candidate
}

internal fun <T> selectAccessibilityWindowRoot(
    listedRoot: T?,
    activeRoot: T?,
    windowId: Int,
    active: Boolean,
    focused: Boolean,
    rootWindowId: (T) -> Int,
): T? = listedRoot ?: activeRoot?.takeIf { candidate ->
    (active || focused) && rootWindowId(candidate) == windowId
}

private fun TargetBounds.contains(other: TargetBounds): Boolean =
    other.left >= left && other.top >= top && other.right <= right && other.bottom <= bottom

internal const val ACTIVE_CONTENT_WINDOW_TYPE = "active_content"
internal const val APPLICATION_WINDOW_TYPE = "application"

internal fun isApplicationContentWindowType(type: String): Boolean =
    type == APPLICATION_WINDOW_TYPE || type == ACTIVE_CONTENT_WINDOW_TYPE

internal fun isControllerOwnedWindow(
    accessibilityOverlay: Boolean,
    packageName: String?,
    type: String,
    servicePackage: String,
): Boolean = accessibilityOverlay ||
    (packageName == servicePackage && !isApplicationContentWindowType(type))
