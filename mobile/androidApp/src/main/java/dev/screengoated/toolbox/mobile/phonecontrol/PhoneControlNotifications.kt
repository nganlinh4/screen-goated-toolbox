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
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized

internal class PhoneControlSessionNotification(
    private val service: Service,
    private val stopIntent: Intent,
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
        val stop = PendingIntent.getService(
            service,
            1,
            stopIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
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
    fun show(context: Context, message: String, continueIntent: Intent) {
        ensurePhoneControlNotificationChannel(context)
        val localized = context.uiLocalized()
        val resume = PendingIntent.getActivity(
            context,
            2,
            continueIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val cancel = PendingIntent.getActivity(
            context,
            3,
            PhoneControlActivity.cancelSetupIntent(context),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val public = publicPhoneControlNotification(context, message)
        val notification = NotificationCompat.Builder(context, PHONE_CONTROL_CHANNEL_ID)
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
        context.getSystemService(NotificationManager::class.java).notify(
            SETUP_NOTIFICATION_ID,
            notification,
        )
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
private const val SESSION_NOTIFICATION_ID = 4081
private const val SETUP_NOTIFICATION_ID = 4083
