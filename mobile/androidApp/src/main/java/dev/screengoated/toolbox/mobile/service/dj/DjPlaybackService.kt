package dev.screengoated.toolbox.mobile.service.dj

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Looper
import androidx.core.app.NotificationCompat
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.session.MediaStyleNotificationHelper
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R

/**
 * Foreground service that keeps the DJ audio alive in background and
 * provides media notification controls (play/pause/stop) on the
 * notification shade and lock screen via Media3 MediaSession.
 */
@UnstableApi
class DjPlaybackService : MediaSessionService() {

    private var mediaSession: MediaSession? = null

    override fun onCreate() {
        super.onCreate()
        ensureNotificationChannel()

        val djPlayer = DjWebViewPlayer(Looper.getMainLooper())
        playerRef = djPlayer
        mediaSession = MediaSession.Builder(this, djPlayer)
            .setId("dj-playback")
            .build()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Must call startForeground() immediately to avoid FGS timeout crash.
        // Use MediaStyle so it shows as a proper media player in quick settings.
        val session = mediaSession
        val openAppIntent = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("Be a DJ")
            .setContentText("Playing music")
            .setContentIntent(openAppIntent)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setCategory(NotificationCompat.CATEGORY_TRANSPORT)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)

        if (session != null) {
            builder.setStyle(MediaStyleNotificationHelper.MediaStyle(session))
        }

        startForeground(
            NOTIFICATION_ID,
            builder.build(),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
        )

        return super.onStartCommand(intent, flags, startId)
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return mediaSession
    }

    override fun onDestroy() {
        mediaSession?.run {
            player.release()
            release()
        }
        mediaSession = null
        playerRef = null
        super.onDestroy()
    }

    private fun ensureNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "DJ Playback",
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "Background music playback for Be a DJ"
            setSound(null, null)
            enableVibration(false)
            setShowBadge(false)
        }
        getSystemService(NotificationManager::class.java)
            .createNotificationChannel(channel)
    }

    companion object {
        const val CHANNEL_ID = "sgt_dj_playback"
        const val NOTIFICATION_ID = 1002

        /** Static ref so the JS bridge can push state updates. */
        var playerRef: DjWebViewPlayer? = null
            private set

        fun start(context: Context) {
            context.startForegroundService(
                Intent(context, DjPlaybackService::class.java)
            )
        }

        fun stop(context: Context) {
            context.stopService(
                Intent(context, DjPlaybackService::class.java)
            )
        }
    }
}
