package dev.screengoated.toolbox.mobile.updater

import android.content.Context
import okhttp3.OkHttpClient

internal fun createAppUpdateController(
    _context: Context,
    httpClient: OkHttpClient,
): AppUpdateController = AppUpdateRepository(httpClient)
