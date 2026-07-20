package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.media.projection.MediaProjectionManager
import android.app.role.RoleManager
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSessionRegistry

internal enum class AccessibilityTargetAuthority(val wireName: String) {
    ROUTINE("routine"),
    CONSEQUENTIAL("consequential"),
    OS_OWNED_USER_STEP("os_owned_user_step"),
}

internal fun strongestAccessibilityAuthority(
    first: AccessibilityTargetAuthority,
    second: AccessibilityTargetAuthority,
): AccessibilityTargetAuthority = if (first.ordinal >= second.ordinal) first else second

internal fun structuralNodeAuthority(
    supportsPlatformDismiss: Boolean,
): AccessibilityTargetAuthority = if (supportsPlatformDismiss) {
    AccessibilityTargetAuthority.CONSEQUENTIAL
} else {
    AccessibilityTargetAuthority.ROUTINE
}

internal data class AccessibilityTargetAuthorityPolicy(
    val osOwnedUserStepPackages: Set<String>,
    val osOwnedOverlayCandidatePackages: Set<String> = emptySet(),
    val platformUserStepActive: Boolean = false,
) {
    fun classify(packageName: String): AccessibilityTargetAuthority =
        if (packageName in osOwnedUserStepPackages) {
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP
        } else {
            AccessibilityTargetAuthority.ROUTINE
        }

    fun <T> classifyWindow(
        window: CapturedAccessibilityWindow<T>,
        windows: List<CapturedAccessibilityWindow<T>>,
    ): AccessibilityTargetAuthority {
        classify(window.packageName.orEmpty()).takeIf {
            it == AccessibilityTargetAuthority.OS_OWNED_USER_STEP
        }?.let { return it }
        val packageName = window.packageName?.takeIf(String::isNotBlank)
            ?: return AccessibilityTargetAuthority.ROUTINE
        if (platformUserStepActive &&
            window.type in USER_STEP_WINDOW_TYPES &&
            (window.active || window.focused)
        ) {
            return AccessibilityTargetAuthority.OS_OWNED_USER_STEP
        }
        val overlaysAnotherApplication = window.type == APPLICATION_WINDOW &&
            (window.active || window.focused) &&
            packageName in osOwnedOverlayCandidatePackages &&
            windows.any { behind ->
                behind.displayId == window.displayId &&
                    behind.layer < window.layer &&
                    behind.type == APPLICATION_WINDOW &&
                    !behind.accessibilityOverlay &&
                    !behind.packageName.isNullOrBlank() &&
                    behind.packageName != packageName
            }
        return if (overlaysAnotherApplication) {
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP
        } else {
            AccessibilityTargetAuthority.ROUTINE
        }
    }
}

internal fun resolveAccessibilityTargetAuthorityPolicy(
    context: Context,
): AccessibilityTargetAuthorityPolicy {
    val packageManager = context.packageManager
    val dedicatedPackages = platformConfirmationIntents(context)
        .mapNotNull { intent -> resolveUniqueSystemHandler(packageManager, intent) }
        .toSet()
    val overlayCandidatePackages = buildSet {
        resolveUniqueSystemHandler(
            packageManager,
            Intent(ACTION_MANAGE_PERMISSIONS),
        )?.let(::add)
        context.getSystemService(RoleManager::class.java)
            ?.createRequestRoleIntent(RoleManager.ROLE_BROWSER)
            ?.`package`
            ?.let { packageName -> systemPackageOrNull(packageManager, packageName) }
            ?.let(::add)
        context.getSystemService(MediaProjectionManager::class.java)
            ?.createScreenCaptureIntent()
            ?.component
            ?.packageName
            ?.let { packageName -> systemPackageOrNull(packageManager, packageName) }
            ?.let(::add)
    }
    return AccessibilityTargetAuthorityPolicy(
        osOwnedUserStepPackages = dedicatedPackages,
        osOwnedOverlayCandidatePackages = overlayCandidatePackages,
        platformUserStepActive = PlatformUserStepSessionRegistry.hasActiveSession(),
    )
}

private fun platformConfirmationIntents(context: Context): List<Intent> {
    val packageUri = Uri.parse("package:${context.packageName}")
    val installUri = Uri.parse("content://${context.packageName}.phonecontrol/probe.apk")
    return listOf(
        Intent(ACTION_UNINSTALL_PACKAGE, packageUri),
        Intent(ACTION_INSTALL_PACKAGE).setDataAndType(installUri, APK_MIME),
    )
}

private fun resolveUniqueSystemHandler(
    packageManager: PackageManager,
    intent: Intent,
): String? = queryIntentActivities(packageManager, intent)
    .asSequence()
    .mapNotNull { result -> result.activityInfo }
    .filter { activity -> activity.applicationInfo.isSystemAuthority() }
    .map { activity -> activity.packageName }
    .filter(String::isNotBlank)
    .distinct()
    .singleOrNull()

@Suppress("DEPRECATION")
private fun queryIntentActivities(
    packageManager: PackageManager,
    intent: Intent,
) = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
    packageManager.queryIntentActivities(intent, PackageManager.ResolveInfoFlags.of(0))
} else {
    packageManager.queryIntentActivities(intent, 0)
}

private fun ApplicationInfo.isSystemAuthority(): Boolean =
    flags and (ApplicationInfo.FLAG_SYSTEM or ApplicationInfo.FLAG_UPDATED_SYSTEM_APP) != 0

@Suppress("DEPRECATION")
private fun systemPackageOrNull(
    packageManager: PackageManager,
    packageName: String,
): String? = runCatching {
    val info = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        packageManager.getApplicationInfo(packageName, PackageManager.ApplicationInfoFlags.of(0))
    } else {
        packageManager.getApplicationInfo(packageName, 0)
    }
    packageName.takeIf { info.isSystemAuthority() }
}.getOrNull()

private const val APK_MIME = "application/vnd.android.package-archive"
private const val ACTION_INSTALL_PACKAGE = "android.intent.action.INSTALL_PACKAGE"
private const val ACTION_UNINSTALL_PACKAGE = "android.intent.action.UNINSTALL_PACKAGE"
private const val ACTION_MANAGE_PERMISSIONS = "android.intent.action.MANAGE_PERMISSIONS"
private const val APPLICATION_WINDOW = "application"
private val USER_STEP_WINDOW_TYPES = setOf(APPLICATION_WINDOW, "system")
