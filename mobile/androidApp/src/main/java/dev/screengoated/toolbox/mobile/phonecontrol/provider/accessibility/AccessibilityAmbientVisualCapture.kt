package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.graphics.Rect
import android.hardware.display.DisplayManager
import android.os.Build
import android.view.Display
import android.view.accessibility.AccessibilityWindowInfo
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds

internal data class AccessibilityAmbientWindowScreenshot(
    val screenshot: AccessibilityScreenshot,
    val displayId: Int,
    val packageOrSurface: String,
    val rotation: Int,
    val densityDpi: Int,
)

private data class AmbientWindowTarget(
    val id: Int,
    val displayId: Int,
    val layer: Int,
    val packageOrSurface: String,
    val bounds: TargetBounds,
    val active: Boolean,
    val focused: Boolean,
)

internal suspend fun captureAmbientExternalWindowScreenshot():
    AccessibilityProviderResult<AccessibilityAmbientWindowScreenshot> {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        return AccessibilityProviderResult.Failure(
            code = "window_screenshot_unsupported",
            message = "Window-scoped screenshots require Android 14 or newer.",
            retryable = false,
        )
    }
    val target = when (
        val resolved = PhoneControlAccessibilityProvider.onServiceMain { service ->
            resolveAmbientWindowTarget(service)
        }
    ) {
        is AccessibilityProviderResult.Failure -> return resolved
        is AccessibilityProviderResult.Success -> resolved.value
    }
    val screenshot = when (
        val captured = PhoneControlAccessibilityProvider.screenshotWindowOnly(
            target.id.toLong(),
            target.bounds,
        )
    ) {
        is AccessibilityProviderResult.Failure -> return captured
        is AccessibilityProviderResult.Success -> captured.value
    }
    return AccessibilityProviderResult.Success(
        AccessibilityAmbientWindowScreenshot(
            screenshot = screenshot.copy(captureProvider = AMBIENT_WINDOW_PROVIDER),
            displayId = target.displayId,
            packageOrSurface = target.packageOrSurface,
            rotation = target.rotation,
            densityDpi = target.densityDpi,
        ),
    )
}

private fun resolveAmbientWindowTarget(
    service: dev.screengoated.toolbox.mobile.service.SgtAccessibilityService,
): AccessibilityProviderResult<AmbientWindowTargetWithDisplay> {
    val target = accessibilityWindows(service)
        .asSequence()
        .filter { entry -> entry.displayId == Display.DEFAULT_DISPLAY }
        .mapNotNull { entry -> entry.toAmbientTarget(service.packageName) }
        .sortedWith(
            compareByDescending<AmbientWindowTarget> { it.active || it.focused }
                .thenByDescending { it.active }
                .thenByDescending { it.focused }
                .thenByDescending { it.layer },
        )
        .firstOrNull()
        ?: return AccessibilityProviderResult.Failure(
            code = "surface_unavailable",
            message = "No external visual surface is currently observable.",
            retryable = true,
            freshObservationRequired = true,
        )
    val display = service.getSystemService(DisplayManager::class.java)
        ?.getDisplay(target.displayId)
        ?: return AccessibilityProviderResult.Failure(
            code = "unsupported_display",
            message = "The current visual display is unavailable.",
            retryable = true,
        )
    val density = service.createDisplayContext(display).resources.displayMetrics.densityDpi
    return AccessibilityProviderResult.Success(
        AmbientWindowTargetWithDisplay(
            id = target.id,
            displayId = target.displayId,
            packageOrSurface = target.packageOrSurface,
            bounds = target.bounds,
            rotation = display.rotation,
            densityDpi = density,
        ),
    )
}

private fun AccessibilityWindowOnDisplay<AccessibilityWindowInfo>.toAmbientTarget(
    servicePackage: String,
): AmbientWindowTarget? {
    val type = windowTypeName(window.type)
    if (type != APPLICATION_WINDOW_TYPE && type != "system") return null
    val root = window.root
    val packageName = root?.packageName?.toString()
    if (packageName == servicePackage && type != APPLICATION_WINDOW_TYPE) return null
    val bounds = Rect().also(window::getBoundsInScreen).toTargetBoundsOrNull() ?: return null
    return AmbientWindowTarget(
        id = window.id,
        displayId = displayId,
        layer = window.layer,
        packageOrSurface = packageName?.takeIf(String::isNotBlank)
            ?: window.title?.toString()?.takeIf(String::isNotBlank)
            ?: "android-window-${window.id}",
        bounds = bounds,
        active = window.isActive,
        focused = window.isFocused,
    )
}

private data class AmbientWindowTargetWithDisplay(
    val id: Int,
    val displayId: Int,
    val packageOrSurface: String,
    val bounds: TargetBounds,
    val rotation: Int,
    val densityDpi: Int,
)

internal const val AMBIENT_WINDOW_PROVIDER = "accessibility_window_lease_free"
