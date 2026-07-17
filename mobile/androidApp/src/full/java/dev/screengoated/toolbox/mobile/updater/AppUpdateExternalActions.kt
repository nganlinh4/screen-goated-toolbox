package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import android.content.Intent
import androidx.core.net.toUri
fun openAppUpdate(context: Context, state: AppUpdateUiState): Boolean {
    val target = state.actionUrl ?: return false
    return launchView(context, target)
}

private fun launchView(context: Context, url: String): Boolean {
    val intent = Intent(Intent.ACTION_VIEW, url.toUri()).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    return runCatching {
        context.startActivity(intent)
    }.isSuccess
}
