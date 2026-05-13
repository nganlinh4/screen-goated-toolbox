package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.content.ContextCompat

internal fun tryStartForegroundService(
    context: Context,
    intent: Intent,
    logTag: String,
): Boolean {
    return try {
        ContextCompat.startForegroundService(context, intent)
        true
    } catch (error: Exception) {
        Log.e(logTag, "Foreground service start failed for ${intent.component?.className}", error)
        false
    }
}
