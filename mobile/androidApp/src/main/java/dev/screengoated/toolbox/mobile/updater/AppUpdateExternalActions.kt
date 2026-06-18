package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import android.content.Intent
import androidx.core.net.toUri
import dev.screengoated.toolbox.mobile.BuildConfig

private const val PLAY_PACKAGE = "dev.screengoated.toolbox.mobile"

fun openAppUpdate(context: Context, state: AppUpdateUiState): Boolean {
    // Play-distributed builds update through Google Play, not a GitHub .apk.
    // The "play" flavor opens the Play Store listing; the "full" sideload flavor
    // falls back to the GitHub release asset/page.
    if (BuildConfig.FLAVOR == "play") {
        return openPlayStoreListing(context)
    }
    val target = state.actionUrl ?: return false
    return launchView(context, target)
}

private fun openPlayStoreListing(context: Context): Boolean {
    // Prefer the Play Store app, fall back to the web listing.
    if (launchView(context, "market://details?id=$PLAY_PACKAGE")) {
        return true
    }
    return launchView(context, "https://play.google.com/store/apps/details?id=$PLAY_PACKAGE")
}

private fun launchView(context: Context, url: String): Boolean {
    val intent = Intent(Intent.ACTION_VIEW, url.toUri()).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    return runCatching {
        context.startActivity(intent)
    }.isSuccess
}
