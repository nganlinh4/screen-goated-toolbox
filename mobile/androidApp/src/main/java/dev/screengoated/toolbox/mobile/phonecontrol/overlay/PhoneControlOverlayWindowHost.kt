package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.content.Context
import android.hardware.input.InputManager
import android.os.Build
import android.provider.Settings
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService

internal class PhoneControlOverlayWindowHost private constructor(
    val context: Context,
    val windowManager: WindowManager,
    val windowType: Int,
    val rendererAlpha: Float,
    private val accessibilityOwner: SgtAccessibilityService?,
) {
    val trusted: Boolean
        get() = accessibilityOwner != null

    fun isAvailable(): Boolean = accessibilityOwner?.let {
        SgtAccessibilityService.instance === it
    } ?: Settings.canDrawOverlays(context)

    fun sameOwner(other: PhoneControlOverlayWindowHost): Boolean =
        accessibilityOwner === other.accessibilityOwner && windowType == other.windowType

    fun describe(): String = if (trusted) "accessibility_trusted" else "application_fallback"

    companion object {
        fun resolve(baseContext: Context): PhoneControlOverlayWindowHost {
            SgtAccessibilityService.instance?.let { service ->
                return PhoneControlOverlayWindowHost(
                    context = service,
                    windowManager = service.getSystemService(WindowManager::class.java),
                    windowType = WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
                    rendererAlpha = 1f,
                    accessibilityOwner = service,
                )
            }
            val context = baseContext.applicationContext
            val maximumAlpha = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                context.getSystemService(InputManager::class.java)
                    .maximumObscuringOpacityForTouch
            } else {
                1f
            }
            return PhoneControlOverlayWindowHost(
                context = context,
                windowManager = context.getSystemService(WindowManager::class.java),
                windowType = WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                rendererAlpha = maximumAlpha.coerceIn(0f, 1f),
                accessibilityOwner = null,
            )
        }
    }
}
