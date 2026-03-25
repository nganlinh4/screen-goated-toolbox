package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import android.content.Intent
import android.net.Uri

fun openAppUpdate(context: Context, state: AppUpdateUiState) {
    val target = state.actionUrl ?: return
    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(target)).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    context.startActivity(intent)
}
