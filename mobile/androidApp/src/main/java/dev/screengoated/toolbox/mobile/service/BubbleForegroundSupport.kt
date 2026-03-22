package dev.screengoated.toolbox.mobile.service

import android.app.Notification
import android.content.pm.ServiceInfo
import android.os.Build
import android.util.Log
import dev.screengoated.toolbox.mobile.service.preset.PresetAudioForegroundMode

internal fun applyBubbleForegroundMode(
    service: BubbleService,
    mode: PresetAudioForegroundMode,
    notification: Notification,
) {
    val serviceType = resolveBubbleForegroundServiceType(mode)
    if (service.currentAudioForegroundMode == mode) {
        return
    }
    try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            service.startForeground(BubbleService.NOTIFICATION_ID, notification, serviceType)
        } else {
            service.startForeground(BubbleService.NOTIFICATION_ID, notification)
        }
        service.currentAudioForegroundMode = mode
    } catch (error: SecurityException) {
        Log.e(BUBBLE_FOREGROUND_TAG, "Bubble foreground mode update failed: mode=$mode", error)
        if (mode != PresetAudioForegroundMode.NONE) {
            applyBubbleForegroundMode(service, PresetAudioForegroundMode.NONE, notification)
        }
    }
}

internal fun resolveBubbleForegroundServiceType(mode: PresetAudioForegroundMode): Int {
    return when (mode) {
        PresetAudioForegroundMode.NONE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        PresetAudioForegroundMode.MICROPHONE -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        PresetAudioForegroundMode.MEDIA_PROJECTION -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
    }
}

private const val BUBBLE_FOREGROUND_TAG = "BubbleForeground"
