package dev.screengoated.toolbox.mobile.service.dj

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.IBinder
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.service.tryStartForegroundService
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized

/**
 * Foreground service that keeps the DJ audio alive in background and
 * provides media notification controls (play/pause/stop) on the
 * notification shade and lock screen through Android's platform media session.
 */
class DjPlaybackService : Service() {

    private lateinit var mediaSession: MediaSession
    private var playbackState = PlaybackState.STATE_PAUSED
    private var currentTitle: String? = null

    override fun onCreate() {
        super.onCreate()
        ensureNotificationChannel()
        mediaSession = MediaSession(this, "dj-playback").apply {
            setCallback(object : MediaSession.Callback() {
                override fun onPlay() {
                    updateSession(PlaybackState.STATE_BUFFERING, null)
                    DjWebViewHolder.onPlayFromNotification?.invoke()
                }

                override fun onPause() {
                    updateSession(PlaybackState.STATE_PAUSED, null)
                    DjWebViewHolder.onPauseFromNotification?.invoke()
                }

                override fun onStop() {
                    updateSession(PlaybackState.STATE_STOPPED, null)
                    DjWebViewHolder.onStopFromNotification?.invoke()
                    stopForeground(STOP_FOREGROUND_REMOVE)
                    stopSelf()
                }
            })
            setSessionActivity(openAppPendingIntent())
            isActive = true
        }
        publishPlaybackState()
        publishMetadata()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_UPDATE -> updateSession(
                webState(intent.getStringExtra(EXTRA_STATE)),
                intent.getStringExtra(EXTRA_TITLE),
            )
            ACTION_PLAY -> mediaSession.controller.transportControls.play()
            ACTION_PAUSE -> mediaSession.controller.transportControls.pause()
            ACTION_STOP -> mediaSession.controller.transportControls.stop()
            else -> publishNotification()
        }
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun publishNotification() {
        val l10n = uiLocalized()
        val isPlaying = playbackState == PlaybackState.STATE_PLAYING ||
            playbackState == PlaybackState.STATE_BUFFERING
        val toggleAction = if (isPlaying) {
            notificationAction(
                R.drawable.ms_pause,
                l10n.getString(R.string.notification_action_pause),
                ACTION_PAUSE,
                REQUEST_PAUSE,
            )
        } else {
            notificationAction(
                R.drawable.ms_play_arrow,
                l10n.getString(R.string.notification_action_play),
                ACTION_PLAY,
                REQUEST_PLAY,
            )
        }
        val stopAction = notificationAction(
            R.drawable.ms_stop,
            l10n.getString(R.string.notification_action_stop),
            ACTION_STOP,
            REQUEST_STOP,
        )
        val notification = Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle(
                currentTitle ?: l10n.getString(R.string.dj_notification_title),
            )
            .setContentText(l10n.getString(R.string.dj_notification_text))
            .setContentIntent(openAppPendingIntent())
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(Notification.CATEGORY_TRANSPORT)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(toggleAction)
            .addAction(stopAction)
            .setStyle(
                Notification.MediaStyle()
                    .setMediaSession(mediaSession.sessionToken)
                    .setShowActionsInCompactView(0, 1),
            )
            .build()

        startForeground(
            NOTIFICATION_ID,
            notification,
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
        )
    }

    private fun notificationAction(
        icon: Int,
        title: CharSequence,
        action: String,
        requestCode: Int,
    ): Notification.Action = Notification.Action.Builder(
        Icon.createWithResource(this, icon),
        title,
        PendingIntent.getService(
            this,
            requestCode,
            Intent(this, DjPlaybackService::class.java).setAction(action),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        ),
    ).build()

    private fun openAppPendingIntent(): PendingIntent = PendingIntent.getActivity(
        this,
        REQUEST_OPEN,
        Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun updateSession(state: Int, title: String?) {
        playbackState = state
        if (!title.isNullOrBlank()) currentTitle = title
        publishPlaybackState()
        publishMetadata()
        publishNotification()
    }

    private fun publishPlaybackState() {
        val actions = PlaybackState.ACTION_STOP or if (
            playbackState == PlaybackState.STATE_PLAYING ||
            playbackState == PlaybackState.STATE_BUFFERING
        ) {
            PlaybackState.ACTION_PAUSE
        } else {
            PlaybackState.ACTION_PLAY
        }
        mediaSession.setPlaybackState(
            PlaybackState.Builder()
                .setActions(actions)
                .setState(playbackState, PlaybackState.PLAYBACK_POSITION_UNKNOWN, 1f)
                .build(),
        )
    }

    private fun publishMetadata() {
        val l10n = uiLocalized()
        mediaSession.setMetadata(
            MediaMetadata.Builder()
                .putString(
                    MediaMetadata.METADATA_KEY_TITLE,
                    currentTitle ?: l10n.getString(R.string.dj_notification_title),
                )
                .putString(MediaMetadata.METADATA_KEY_ARTIST, "Screen Goated Toolbox")
                .build(),
        )
    }

    override fun onDestroy() {
        mediaSession.isActive = false
        mediaSession.release()
        super.onDestroy()
    }

    private fun ensureNotificationChannel() {
        val l10n = uiLocalized()
        val channel = NotificationChannel(
            CHANNEL_ID,
            l10n.getString(R.string.dj_playback_channel_name),
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = l10n.getString(R.string.dj_playback_channel_description)
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

        private const val ACTION_UPDATE = "dev.screengoated.toolbox.dj.UPDATE"
        private const val ACTION_PLAY = "dev.screengoated.toolbox.dj.PLAY"
        private const val ACTION_PAUSE = "dev.screengoated.toolbox.dj.PAUSE"
        private const val ACTION_STOP = "dev.screengoated.toolbox.dj.STOP"
        private const val EXTRA_STATE = "state"
        private const val EXTRA_TITLE = "title"
        private const val REQUEST_OPEN = 0
        private const val REQUEST_PLAY = 1
        private const val REQUEST_PAUSE = 2
        private const val REQUEST_STOP = 3

        fun update(context: Context, state: String, title: String? = null): Boolean {
            return tryStartForegroundService(
                context,
                Intent(context, DjPlaybackService::class.java)
                    .setAction(ACTION_UPDATE)
                    .putExtra(EXTRA_STATE, state)
                    .putExtra(EXTRA_TITLE, title),
                "DjPlaybackService",
            )
        }

        fun stop(context: Context) {
            val intent = Intent(context, DjPlaybackService::class.java)
            context.stopService(intent)
        }

        private fun webState(state: String?): Int = when (state) {
            "playing" -> PlaybackState.STATE_PLAYING
            "loading" -> PlaybackState.STATE_BUFFERING
            "paused" -> PlaybackState.STATE_PAUSED
            else -> PlaybackState.STATE_STOPPED
        }
    }
}
