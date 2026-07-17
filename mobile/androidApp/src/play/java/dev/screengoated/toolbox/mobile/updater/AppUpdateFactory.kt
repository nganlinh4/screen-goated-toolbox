package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import okhttp3.OkHttpClient

internal fun createAppUpdateController(
    context: Context,
    httpClient: OkHttpClient,
): AppUpdateController = PlayInAppUpdateManager(context)
