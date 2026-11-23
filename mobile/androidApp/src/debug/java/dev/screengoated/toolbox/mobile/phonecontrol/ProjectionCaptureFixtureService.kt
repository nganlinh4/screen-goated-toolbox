package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.IBinder
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionGrant
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionProvider

class ProjectionCaptureFixtureService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Projection acceptance", NotificationManager.IMPORTANCE_LOW))
        startForeground(2197, Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_qs_tile)
            .setContentTitle("SGT debug projection acceptance").build())
        @Suppress("DEPRECATION")
        val data = intent?.getParcelableExtra<Intent>("resultData")
        val grant = PhoneControlProjectionGrant.fromActivityResult(intent?.getIntExtra("resultCode", 0) ?: 0, data)
        if (grant == null) stopSelf() else PhoneControlProjectionProvider.start(this, grant) { stopSelf() }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        PhoneControlProjectionProvider.stop()
        stopForeground(STOP_FOREGROUND_REMOVE)
        getSystemService(NotificationManager::class.java).deleteNotificationChannel(CHANNEL)
        super.onDestroy()
    }

    companion object { private const val CHANNEL = "sgt_projection_acceptance" }
}
