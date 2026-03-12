package dev.screengoated.toolbox.mobile.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.media.AudioAttributes
import android.net.Uri
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase

class ServiceNotificationFactory(
    private val context: Context,
) {
    private val notificationManager =
        context.getSystemService(NotificationManager::class.java)

    fun ensureChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            context.getString(R.string.live_translate_channel_name),
            NotificationManager.IMPORTANCE_MIN,
        ).apply {
            description = context.getString(R.string.live_translate_channel_description)
            setSound(null as Uri?, null as AudioAttributes?)
            enableVibration(false)
            setShowBadge(false)
            lockscreenVisibility = Notification.VISIBILITY_SECRET
        }
        notificationManager.createNotificationChannel(channel)
    }

    fun snapshot(state: LiveSessionState): ServiceNotificationSnapshot {
        val contentText = when (state.phase) {
            SessionPhase.AWAITING_PERMISSIONS -> "Waiting for permissions"
            SessionPhase.STARTING,
            SessionPhase.LISTENING,
            SessionPhase.TRANSLATING,
            -> "Live translate is running"
            SessionPhase.ERROR -> state.lastError?.trim()?.take(80).orEmpty().ifBlank { "Live translate stopped" }
            SessionPhase.STOPPED -> "Live translate stopped"
            SessionPhase.IDLE -> context.getString(R.string.live_translate_notification_text)
        }
        return ServiceNotificationSnapshot(
            title = "SGT Live Translate",
            contentText = contentText,
        )
    }

    fun build(snapshot: ServiceNotificationSnapshot): Notification {
        val openAppIntent = PendingIntent.getActivity(
            context,
            1,
            Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stopIntent = PendingIntent.getService(
            context,
            2,
            Intent(context, LiveTranslateService::class.java).setAction(LiveTranslateService.ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        return NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setContentTitle(snapshot.title)
            .setContentText(snapshot.contentText)
            .setContentIntent(openAppIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setLocalOnly(true)
            .setShowWhen(false)
            .addAction(0, "Stop", stopIntent)
            .build()
    }

    companion object {
        const val CHANNEL_ID = "sgt_live_translate_silent_v2"
        const val NOTIFICATION_ID = 1001
    }
}

data class ServiceNotificationSnapshot(
    val title: String,
    val contentText: String,
)
