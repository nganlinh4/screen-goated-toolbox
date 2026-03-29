package dev.screengoated.toolbox.mobile.bilingualrelay

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

class BilingualRelayNotificationFactory(
    private val context: Context,
    private val localeProvider: () -> MobileLocaleText,
) {
    fun ensureChannel() {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(CHANNEL_ID) != null) {
            return
        }
        val locale = localeProvider()
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                locale.bilingualRelayNotificationChannel,
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = locale.bilingualRelayNotificationDescription
            },
        )
    }

    fun build(state: BilingualRelayState): android.app.Notification {
        val locale = localeProvider()
        val stopIntent = PendingIntent.getService(
            context,
            0,
            Intent(context, BilingualRelayService::class.java).setAction(BilingualRelayService.ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val openIntent = PendingIntent.getActivity(
            context,
            1,
            Intent(context, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        val status = when (state.connectionState) {
            BilingualRelayConnectionState.NOT_CONFIGURED -> locale.bilingualRelayStatusNotConfigured
            BilingualRelayConnectionState.CONNECTING -> locale.bilingualRelayStatusConnecting
            BilingualRelayConnectionState.READY -> locale.bilingualRelayStatusReady
            BilingualRelayConnectionState.RECONNECTING -> locale.bilingualRelayStatusReconnecting
            BilingualRelayConnectionState.ERROR -> state.lastError ?: locale.bilingualRelayConnectionLost
            BilingualRelayConnectionState.STOPPED -> locale.bilingualRelayStatusStopped
        }

        return NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(locale.bilingualRelayTitle)
            .setContentText(status)
            .setOngoing(state.isRunning)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setContentIntent(openIntent)
            .addAction(0, locale.bilingualRelayNotificationStop, stopIntent)
            .build()
    }

    companion object {
        const val NOTIFICATION_ID = 42067
        private const val CHANNEL_ID = "sgt_bilingual_relay"
    }
}
