package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import dev.screengoated.toolbox.mobile.SgtMobileApplication

/**
 * App-scoped [DownloaderRepository], full flavor only.
 *
 * The downloader ships solely on the sideload distribution, so it is owned here rather
 * than by AppContainer — the Play flavor carries none of this code. Built on first use
 * (opening the tools card or the downloader screen) instead of at startup.
 */
internal object DownloaderHolder {
    @Volatile
    private var instance: DownloaderRepository? = null

    fun get(context: Context): DownloaderRepository {
        instance?.let { return it }
        return synchronized(this) {
            instance ?: run {
                val appContext = context.applicationContext
                val container = (appContext as SgtMobileApplication).appContainer
                DownloaderRepository(appContext, DownloaderPersistence(appContext, container.json))
                    .also {
                        it.checkTools()
                        instance = it
                    }
            }
        }
    }
}
