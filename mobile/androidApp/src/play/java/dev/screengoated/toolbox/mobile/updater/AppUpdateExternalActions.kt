package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import android.content.Intent
import androidx.core.net.toUri

private const val PLAY_PACKAGE = "dev.screengoated.toolbox.mobile"

fun openAppUpdate(context: Context, _state: AppUpdateUiState): Boolean {
    if (launchView(context, "market://details?id=$PLAY_PACKAGE")) return true
    return launchView(context, "https://play.google.com/store/apps/details?id=$PLAY_PACKAGE")
}

private fun launchView(context: Context, url: String): Boolean {
    val intent = Intent(Intent.ACTION_VIEW, url.toUri()).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    return runCatching { context.startActivity(intent) }.isSuccess
}
