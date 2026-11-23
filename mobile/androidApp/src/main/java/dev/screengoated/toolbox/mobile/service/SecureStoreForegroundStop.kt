package dev.screengoated.toolbox.mobile.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.pm.ServiceInfo
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import dev.screengoated.toolbox.mobile.R

private const val LEGACY_FOREGROUND_TYPE_NONE = 0

/** Settle an already accepted foreground start without opening secure settings or devices. */
internal fun Service.stopForUnavailableSecureStore(startId: Int? = null) {
    settleUnavailableForeground(promote = {
        val channelId = "secure_settings_unavailable"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel(channelId, getString(R.string.secure_store_unavailable_title), NotificationManager.IMPORTANCE_LOW),
            )
        }
        val notification = NotificationCompat.Builder(this, channelId)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle(getString(R.string.secure_store_unavailable_title))
            .setContentText(getString(R.string.secure_store_unavailable_body))
            .setOngoing(false)
            .build()
        val id = 0x70000000 or (javaClass.name.hashCode() and 0x0fffffff)
        startPermissionFreeForeground(id, notification)
    }, remove = { stopForeground(Service.STOP_FOREGROUND_REMOVE) },
        stop = { if (startId == null) stopSelf() else stopSelf(startId) },
        failure = { error -> Log.e("SecureStoreGate", "Unable to settle a foreground service start", error) },
    )
}

/**
 * Promotes an accepted foreground start with a type that needs no runtime
 * permission (shortService on API 34+, untyped before), for paths that only
 * settle the start and stop.
 */
internal fun Service.startPermissionFreeForeground(id: Int, notification: Notification) {
    val type = permissionFreeForegroundType(Build.VERSION.SDK_INT)
    if (type == null) startForeground(id, notification) else startForeground(id, notification, type)
}

internal fun permissionFreeForegroundType(sdkInt: Int): Int? = when {
    sdkInt >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_SHORT_SERVICE
    sdkInt >= Build.VERSION_CODES.Q -> LEGACY_FOREGROUND_TYPE_NONE
    else -> null
}

internal fun settleUnavailableForeground(
    promote: () -> Unit,
    remove: () -> Unit,
    stop: () -> Unit,
    failure: (Exception) -> Unit,
) {
    try {
        promote()
        remove()
    } catch (error: Exception) {
        failure(error)
    } finally {
        stop()
    }
}
