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
            decodeDownloaderSettings(json, configFile.takeIf(File::exists)?.readText())
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

internal fun decodeDownloaderSettings(json: Json, saved: String?): DownloaderSettings =
    if (saved == null) DownloaderSettings(lastVideoFormat = DEFAULT_VIDEO_QUALITY)
    else json.decodeFromString<DownloaderSettings>(saved)
