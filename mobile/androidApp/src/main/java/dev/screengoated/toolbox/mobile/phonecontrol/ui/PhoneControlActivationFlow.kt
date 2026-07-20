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
    START("start"),
}

internal data class PhoneControlActivationSnapshot(
    val apiKeyReady: Boolean,
    val microphoneReady: Boolean,
    val notificationsReady: Boolean,
    val notificationPrompted: Boolean,
    val accessibilityEnabled: Boolean,
    val overlayReady: Boolean,
)

internal fun nextPhoneControlActivationStep(
    snapshot: PhoneControlActivationSnapshot,
): PhoneControlActivationStep = when {
    !snapshot.apiKeyReady -> PhoneControlActivationStep.GEMINI_API
    !snapshot.microphoneReady ||
        (!snapshot.notificationsReady && !snapshot.notificationPrompted) ->
        PhoneControlActivationStep.RUNTIME_PERMISSIONS
    !snapshot.accessibilityEnabled -> PhoneControlActivationStep.ACCESSIBILITY
    !snapshot.overlayReady -> PhoneControlActivationStep.OVERLAY
    else -> PhoneControlActivationStep.START
}

internal fun probePhoneControlActivation(context: Context): PhoneControlActivationSnapshot {
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
        accessibilityEnabled = isAccessibilityEnabled(context),
        overlayReady = Settings.canDrawOverlays(context),
    )
}

internal fun markPhoneControlNotificationPrompted(context: Context) {
    activationPreferences(context).edit {
        putBoolean(KEY_NOTIFICATION_PROMPTED, true)
    }
}

internal fun isAccessibilityEnabled(context: Context): Boolean {
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
