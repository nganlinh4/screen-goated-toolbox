package dev.screengoated.toolbox.mobile.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.service.preset.PresetAudioForegroundMode

class PresetAudioForegroundService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val mode = intent?.getStringExtra(EXTRA_MODE)
            ?.let(PresetAudioForegroundMode::valueOf)
            ?: PresetAudioForegroundMode.NONE

        if (mode == PresetAudioForegroundMode.NONE) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }

        ensureChannel()
        val notification = buildNotification(mode)
        val serviceType = when (mode) {
            PresetAudioForegroundMode.MEDIA_PROJECTION -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
            PresetAudioForegroundMode.MICROPHONE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
            PresetAudioForegroundMode.NONE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        }

        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                startForeground(NOTIFICATION_ID, notification, serviceType)
            } else {
                startForeground(NOTIFICATION_ID, notification)
            }
            START_STICKY
        } catch (error: SecurityException) {
            Log.e(TAG, "Preset audio foreground start failed: mode=$mode", error)
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            START_NOT_STICKY
        }
    }

    private fun ensureChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(
            CHANNEL_ID,
            "SGT Preset Audio",
            NotificationManager.IMPORTANCE_MIN,
        ).apply {
            description = "Foreground notification for preset audio capture"
            setSound(null as Uri?, null as AudioAttributes?)
            enableVibration(false)
            setShowBadge(false)
            lockscreenVisibility = Notification.VISIBILITY_SECRET
        }
        manager.createNotificationChannel(channel)
    }

    private fun buildNotification(mode: PresetAudioForegroundMode): Notification {
        val openAppIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val text = when (mode) {
            PresetAudioForegroundMode.MEDIA_PROJECTION -> "Capturing device audio for preset"
            PresetAudioForegroundMode.MICROPHONE -> "Capturing microphone audio for preset"
            PresetAudioForegroundMode.NONE -> "Preset audio capture idle"
        }
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("SGT Audio Preset")
            .setContentText(text)
            .setContentIntent(openAppIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setLocalOnly(true)
            .setShowWhen(false)
            .build()
    }

    companion object {
        private const val TAG = "PresetAudioFgs"
        private const val CHANNEL_ID = "sgt_preset_audio_capture"
        private const val NOTIFICATION_ID = 1003
        private const val EXTRA_MODE = "dev.screengoated.toolbox.mobile.extra.PRESET_AUDIO_FGS_MODE"

        internal fun sync(context: Context, mode: PresetAudioForegroundMode) {
            val intent = Intent(context, PresetAudioForegroundService::class.java)
                .putExtra(EXTRA_MODE, mode.name)
            if (mode == PresetAudioForegroundMode.NONE) {
                context.startService(intent)
            } else {
                ContextCompat.startForegroundService(context, intent)
            }
        }

        internal fun stop(context: Context) {
            sync(context, PresetAudioForegroundMode.NONE)
        }
    }
}
