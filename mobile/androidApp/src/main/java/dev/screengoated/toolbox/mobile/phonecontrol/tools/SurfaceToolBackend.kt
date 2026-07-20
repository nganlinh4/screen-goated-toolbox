package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import android.graphics.Point
import android.hardware.display.DisplayManager
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidAppProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.delay

internal interface SurfaceToolBackend {
    val isReady: Boolean
    val observationGeneration: Long

    suspend fun observe(): AccessibilityProviderResult<AccessibilityObservation>

    fun launchPackage(packageName: String): AndroidProviderResult

    fun isPackageLaunchable(packageName: String): Boolean

    fun appLabel(packageName: String): String?

    fun displayBounds(displayId: Int): TargetBounds?

    fun invalidate(reason: String)

    suspend fun globalAction(
        lease: AccessibilitySurfaceLease,
        action: Int,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome>

    suspend fun postconditionPause()
}

internal class AndroidSurfaceToolBackend(
    context: Context,
) : SurfaceToolBackend {
    private val app = AndroidAppProvider(context)
    private val packageManager = context.packageManager
    private val displayManager = context.getSystemService(DisplayManager::class.java)

    override val isReady: Boolean
        get() = PhoneControlAccessibilityProvider.isReady
    override val observationGeneration: Long
        get() = PhoneControlAccessibilityProvider.observationGeneration

    override suspend fun observe(): AccessibilityProviderResult<AccessibilityObservation> =
        PhoneControlAccessibilityProvider.observe()

    override fun launchPackage(packageName: String): AndroidProviderResult = app.launchApp(packageName)

    override fun isPackageLaunchable(packageName: String): Boolean =
        packageName.isNotBlank() && packageManager.getLaunchIntentForPackage(packageName) != null

    override fun appLabel(packageName: String): String? = runCatching {
        packageManager.getApplicationInfo(packageName, 0).loadLabel(packageManager).toString()
    }.getOrNull()

    @Suppress("DEPRECATION")
    override fun displayBounds(displayId: Int): TargetBounds? = runCatching {
        val display = displayManager?.getDisplay(displayId) ?: return@runCatching null
        val size = Point()
        display.getRealSize(size)
        if (size.x <= 0 || size.y <= 0) return@runCatching null
        TargetBounds(0, 0, size.x, size.y)
    }.getOrNull()

    override fun invalidate(reason: String) {
        PhoneControlAccessibilityProvider.invalidate(reason)
    }

    override suspend fun globalAction(
        lease: AccessibilitySurfaceLease,
        action: Int,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        PhoneControlAccessibilityProvider.globalAction(lease, action)

    override suspend fun postconditionPause() {
        delay(POSTCONDITION_POLL_MS)
    }
}

private const val POSTCONDITION_POLL_MS = 100L
