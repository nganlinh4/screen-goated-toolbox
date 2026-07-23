package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.Manifest
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.ContextCompat
import androidx.core.content.edit
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService

internal enum class PhoneControlActivationStep(val wireName: String) {
    GEMINI_API("gemini_api"),
    RUNTIME_PERMISSIONS("runtime_permissions"),
    ACCESSIBILITY("accessibility"),
    OVERLAY("overlay"),
    MEDIA_PROJECTION("media_projection"),
    START("start"),
}

internal data class PhoneControlActivationSnapshot(
    val apiKeyReady: Boolean,
    val microphoneReady: Boolean,
    val notificationsReady: Boolean,
    val notificationPrompted: Boolean,
    val accessibilityReady: Boolean,
    val overlayReady: Boolean,
    val mediaProjectionReady: Boolean,
)

internal fun nextPhoneControlActivationStep(
    snapshot: PhoneControlActivationSnapshot,
): PhoneControlActivationStep = when {
    !snapshot.apiKeyReady -> PhoneControlActivationStep.GEMINI_API
    !snapshot.microphoneReady ||
        (!snapshot.notificationsReady && !snapshot.notificationPrompted) ->
        PhoneControlActivationStep.RUNTIME_PERMISSIONS
    !snapshot.accessibilityReady -> PhoneControlActivationStep.ACCESSIBILITY
    !snapshot.overlayReady -> PhoneControlActivationStep.OVERLAY
    !snapshot.mediaProjectionReady -> PhoneControlActivationStep.MEDIA_PROJECTION
    else -> PhoneControlActivationStep.START
}

internal fun probePhoneControlActivation(
    context: Context,
    mediaProjectionReady: Boolean,
): PhoneControlActivationSnapshot {
    val app = context.applicationContext as SgtMobileApplication
    val notificationReady = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
        ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
    return PhoneControlActivationSnapshot(
        apiKeyReady = app.appContainer.repository.currentApiKey().isNotBlank(),
        microphoneReady = ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.RECORD_AUDIO,
        ) == PackageManager.PERMISSION_GRANTED,
        notificationsReady = notificationReady,
        notificationPrompted = notificationReady || activationPreferences(context)
            .getBoolean(KEY_NOTIFICATION_PROMPTED, false),
        accessibilityReady = isAccessibilityReady(context),
        overlayReady = Settings.canDrawOverlays(context),
        mediaProjectionReady = mediaProjectionReady,
    )
}

internal fun markPhoneControlNotificationPrompted(context: Context) {
    activationPreferences(context).edit {
        putBoolean(KEY_NOTIFICATION_PROMPTED, true)
    }
}

internal enum class PhoneControlAccessibilityState {
    DISABLED,
    RECONNECTING,
    READY,
}

internal fun phoneControlAccessibilityState(
    configured: Boolean,
    serviceBound: Boolean,
): PhoneControlAccessibilityState = when {
    configured && serviceBound -> PhoneControlAccessibilityState.READY
    configured -> PhoneControlAccessibilityState.RECONNECTING
    else -> PhoneControlAccessibilityState.DISABLED
}

internal fun probePhoneControlAccessibilityState(
    context: Context,
): PhoneControlAccessibilityState = phoneControlAccessibilityState(
    configured = isAccessibilityConfigured(context),
    serviceBound = SgtAccessibilityService.isAvailable,
)

internal fun isAccessibilityReady(context: Context): Boolean =
    probePhoneControlAccessibilityState(context) == PhoneControlAccessibilityState.READY

internal fun isAccessibilityConfigured(context: Context): Boolean {
    val expected = "${context.packageName}/${SgtAccessibilityService::class.java.name}"
    val enabled = Settings.Secure.getString(
        context.contentResolver,
        Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
    ).orEmpty()
    return enabled.split(':').any { it.equals(expected, ignoreCase = true) }
}

internal fun overlaySettingsIntent(context: Context): Intent = Intent(
    Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
    Uri.parse("package:${context.packageName}"),
)

internal fun accessibilitySettingsIntent(context: Context): Intent = Intent(
    Settings.ACTION_ACCESSIBILITY_SETTINGS,
).putExtra(
    Intent.EXTRA_COMPONENT_NAME,
    ComponentName(context, SgtAccessibilityService::class.java),
)

private fun activationPreferences(context: Context) = context.getSharedPreferences(
    ACTIVATION_PREFERENCES,
    Context.MODE_PRIVATE,
)

private const val ACTIVATION_PREFERENCES = "phone_control_activation"
private const val KEY_NOTIFICATION_PROMPTED = "notification_prompted"
