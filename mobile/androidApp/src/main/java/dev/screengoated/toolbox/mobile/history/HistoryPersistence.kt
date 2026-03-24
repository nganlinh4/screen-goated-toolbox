package dev.screengoated.toolbox.mobile.history

import android.content.Context
import kotlinx.serialization.json.Json
import java.io.File

internal data class HistoryPaths(
    val rootDir: File,
    val databaseFile: File,
    val settingsFile: File,
    val mediaDir: File,
    val supportsFolderOpen: Boolean,
)

internal class HistoryPersistence(
    context: Context,
    private val json: Json,
) {
    private val paths: HistoryPaths = buildPaths(context.applicationContext)

    fun paths(): HistoryPaths = paths

    fun loadDatabase(): StoredHistoryDatabase {
        return try {
            if (paths.databaseFile.exists()) {
                json.decodeFromString<StoredHistoryDatabase>(paths.databaseFile.readText())
            } else {
                StoredHistoryDatabase()
            }
        } catch (_: Exception) {
            StoredHistoryDatabase()
        }
    }

    fun saveDatabase(database: StoredHistoryDatabase) {
        try {
            ensureDirs()
            paths.databaseFile.writeText(
                json.encodeToString(StoredHistoryDatabase.serializer(), database),
            )
        } catch (_: Exception) {
            // Ignore write failures so the main UI never crashes on storage issues.
        }
    }

    fun loadSettings(): HistorySettings {
        return try {
            if (paths.settingsFile.exists()) {
                val decoded = json.decodeFromString<HistorySettings>(paths.settingsFile.readText())
                val normalized = normalizeHistorySettings(decoded)
                if (normalized != decoded) {
                    saveSettings(normalized)
                }
                normalized
            } else {
                HistorySettings()
            }
        } catch (_: Exception) {
            HistorySettings()
        }
    }

    fun saveSettings(settings: HistorySettings) {
        try {
            ensureDirs()
            paths.settingsFile.writeText(
                json.encodeToString(HistorySettings.serializer(), settings),
            )
        } catch (_: Exception) {
            // Ignore write failures so the main UI never crashes on storage issues.
        }
    }

    fun mediaFile(fileName: String): File = File(paths.mediaDir, fileName)

    private fun ensureDirs() {
        paths.rootDir.mkdirs()
        paths.mediaDir.mkdirs()
    }

    private fun buildPaths(context: Context): HistoryPaths {
        val externalRoot = context.getExternalFilesDir(null)?.resolve("history")
        val rootDir = externalRoot ?: File(context.filesDir, "history")
        val mediaDir = rootDir.resolve("history_media")
        rootDir.mkdirs()
        mediaDir.mkdirs()
        return HistoryPaths(
            rootDir = rootDir,
            databaseFile = rootDir.resolve("history.json"),
            settingsFile = rootDir.resolve("history_settings.json"),
            mediaDir = mediaDir,
            supportsFolderOpen = externalRoot != null &&
                mediaDir.absolutePath.startsWith("/storage/emulated/0/"),
        )
    }
}
