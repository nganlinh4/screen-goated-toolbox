package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import kotlinx.serialization.json.Json
import java.io.File

class DownloaderPersistence(
    context: Context,
    private val json: Json,
) {
    private val configFile = File(context.filesDir, "downloader_settings.json")

    fun load(): DownloaderSettings {
        return try {
            if (configFile.exists()) {
                json.decodeFromString<DownloaderSettings>(configFile.readText())
            } else {
                DownloaderSettings()
            }
        } catch (_: Exception) {
            DownloaderSettings()
        }
    }

    fun save(settings: DownloaderSettings) {
        try {
            configFile.writeText(json.encodeToString(DownloaderSettings.serializer(), settings))
        } catch (_: Exception) {
            // Ignore write failures
        }
    }
}
