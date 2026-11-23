package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.service.ForegroundNotificationIds
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized

internal class PhoneControlSessionNotification(
    private val service: Service,
) {
    init {
        ensurePhoneControlNotificationChannel(service)
    }

    fun enterForeground(message: String) {
        val notification = build(message)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            service.startForeground(
                SESSION_NOTIFICATION_ID,
                notification,
                phoneControlForegroundServiceTypes(Build.VERSION.SDK_INT),
            )
        } else {
            service.startForeground(SESSION_NOTIFICATION_ID, notification)
        }
    }

    fun enterStopForeground(message: String) {
        val notification = build(message)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
            } else {
                0 // Before Android 14, a temporary service needs no foreground type.
            }
            service.startForeground(SESSION_NOTIFICATION_ID, notification, type)
        } else {
            service.startForeground(SESSION_NOTIFICATION_ID, notification)
        }
    }

    fun update(message: String) {
        service.getSystemService(NotificationManager::class.java).notify(
            SESSION_NOTIFICATION_ID,
            build(message),
        )
    }

    private fun build(message: String): Notification {
        val localized = service.uiLocalized()
        val open = PendingIntent.getActivity(
            service,
            0,
            Intent(service, MainActivity::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = phoneControlStopPendingIntent(service)
        val public = publicPhoneControlNotification(service, message)
        return NotificationCompat.Builder(service, PHONE_CONTROL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_qs_tile)
            .setContentTitle(localized.getString(R.string.phone_control_title))
            .setContentText(message)
            .setStyle(NotificationCompat.BigTextStyle().bigText(message))
            .setPublicVersion(public)
            .setContentIntent(open)
            .addAction(0, localized.getString(R.string.notification_action_stop), stop)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
    }
}

internal object PhoneControlSetupNotification {
    fun showStopRetry(context: Context, boundary: String): Boolean =
        PhoneControlProjectionDispatches.publishForCurrentBoundary(boundary) {
            publishStopRetry(context, boundary)
        }

    private fun publishStopRetry(context: Context, boundary: String) {
        ensurePhoneControlNotificationChannel(context)
        val localized = context.uiLocalized()
        val retry = phoneControlStopPendingIntent(context, boundary, requestCode = 6)
        val message = localized.getString(R.string.phone_control_stop_retry)
        val notification = NotificationCompat.Builder(context, PHONE_CONTROL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_qs_tile)
            .setContentTitle(localized.getString(R.string.phone_control_title))
            .setContentText(message)
            .setStyle(NotificationCompat.BigTextStyle().bigText(message))
            .setContentIntent(retry)
            .setAutoCancel(true)
            .build()
        context.getSystemService(NotificationManager::class.java).notify(STOP_RETRY_NOTIFICATION_ID, notification)
    }

    fun clearStopRetry(context: Context) {
        context.getSystemService(NotificationManager::class.java).cancel(STOP_RETRY_NOTIFICATION_ID)
    }

    fun showProjectionBusy(context: Context, message: String, intent: Intent) {
        ensurePhoneControlNotificationChannel(context)
        val retry = PendingIntent.getActivity(
            context, 5, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val localized = context.uiLocalized()
        val notification = NotificationCompat.Builder(context, PHONE_CONTROL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_qs_tile)
            .setContentTitle(localized.getString(R.string.phone_control_title))
            .setContentText(message)
            .setStyle(NotificationCompat.BigTextStyle().bigText(message))
            .setContentIntent(retry)
            .setAutoCancel(true)
            .build()
        context.getSystemService(NotificationManager::class.java)
            .notify(PROJECTION_BUSY_NOTIFICATION_ID, notification)
    }

    fun clearProjectionBusy(context: Context) {
        context.getSystemService(NotificationManager::class.java)
            .cancel(PROJECTION_BUSY_NOTIFICATION_ID)
    }

    fun showOrphanedReentry(context: Context, message: String, intent: Intent): Boolean {
        ensurePhoneControlNotificationChannel(context)
        val manager = context.getSystemService(NotificationManager::class.java)
        if (!NotificationManagerCompat.from(context).areNotificationsEnabled() ||
            manager.getNotificationChannel(PHONE_CONTROL_CHANNEL_ID)?.importance ==
            NotificationManager.IMPORTANCE_NONE
        ) return false
        manager.notify(ORPHAN_REENTRY_NOTIFICATION_ID, build(context, message, intent, 4))
        return true
    }

    fun clearOrphanedReentry(context: Context) {
        context.getSystemService(NotificationManager::class.java)
            .cancel(ORPHAN_REENTRY_NOTIFICATION_ID)
    }

    fun show(context: Context, message: String, continueIntent: Intent?) {
        ensurePhoneControlNotificationChannel(context)
        val notification = build(context, message, continueIntent)
        context.getSystemService(NotificationManager::class.java).notify(
            SETUP_NOTIFICATION_ID,
            notification,
        )
    }

    internal fun build(
        context: Context,
        message: String,
        continueIntent: Intent?,
        requestCode: Int = 2,
    ): Notification {
        val localized = context.uiLocalized()
        val resume = continueIntent?.let {
            PendingIntent.getActivity(
                context,
                requestCode,
                it,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        }
        val cancel = PendingIntent.getActivity(
            context,
            3,
            PhoneControlActivity.cancelSetupIntent(context),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val public = publicPhoneControlNotification(context, message)
        return NotificationCompat.Builder(context, PHONE_CONTROL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_qs_tile)
            .setContentTitle(localized.getString(R.string.phone_control_title))
            .setContentText(message)
            .setStyle(NotificationCompat.BigTextStyle().bigText(message))
            .setPublicVersion(public)
            .setContentIntent(resume)
            .addAction(
                0,
                localized.getString(R.string.phone_control_action_cancel_setup),
                cancel,
            )
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
    }

    fun clear(context: Context) {
        context.getSystemService(NotificationManager::class.java)
            .cancel(SETUP_NOTIFICATION_ID)
    }
}

private fun publicPhoneControlNotification(context: Context, message: String): Notification {
    val localized = context.uiLocalized()
    return NotificationCompat.Builder(context, PHONE_CONTROL_CHANNEL_ID)
        .setSmallIcon(R.drawable.ic_qs_tile)
        .setContentTitle(localized.getString(R.string.phone_control_title))
        .setContentText(message)
        .setStyle(NotificationCompat.BigTextStyle().bigText(message))
        .build()
}

private fun ensurePhoneControlNotificationChannel(context: Context) {
    val localized = context.uiLocalized()
    context.getSystemService(NotificationManager::class.java).createNotificationChannel(
        NotificationChannel(
            PHONE_CONTROL_CHANNEL_ID,
            localized.getString(R.string.phone_control_channel_name),
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = localized.getString(R.string.phone_control_channel_description)
        },
    )
}

internal fun phoneControlForegroundServiceTypes(apiLevel: Int): Int {
    var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK or
        ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
    if (apiLevel >= Build.VERSION_CODES.R) {
        types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
    }
    if (apiLevel >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
    }
    return types
}

private const val PHONE_CONTROL_CHANNEL_ID = "phone_control"
private const val SESSION_NOTIFICATION_ID = ForegroundNotificationIds.PHONE_CONTROL_SESSION
private const val SETUP_NOTIFICATION_ID = ForegroundNotificationIds.PHONE_CONTROL_SETUP
private const val ORPHAN_REENTRY_NOTIFICATION_ID = ForegroundNotificationIds.PHONE_CONTROL_ORPHAN_REENTRY
private const val PROJECTION_BUSY_NOTIFICATION_ID = ForegroundNotificationIds.PHONE_CONTROL_PROJECTION_BUSY
private const val STOP_RETRY_NOTIFICATION_ID = ForegroundNotificationIds.PHONE_CONTROL_STOP_RETRY
