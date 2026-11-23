package dev.screengoated.toolbox.mobile.history

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AndroidFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import kotlinx.serialization.json.Json
import java.io.File
import java.nio.file.Path

internal data class HistoryPaths(
    val rootDir: File,
    val databaseFile: File,
    val settingsFile: File,
    val mediaDir: File,
    val supportsFolderOpen: Boolean,
)

internal class HistoryPersistence internal constructor(
    private val paths: HistoryPaths,
    private val json: Json,
    private val fileIdentity: (Path) -> String = ::androidHistoryFileIdentity,
    mediaWriter: HistoryMediaWriter = HistoryDescriptorMediaWriter(),
    private val fileIo: FileTransactionIo = AndroidFileTransactionIo,
    readOnlySnapshots: Boolean = false,
) : HistoryStorage, AutoCloseable {
    private val atomicFiles = HistoryAtomicFiles(fileIo, readOnlySnapshots = readOnlySnapshots)
    private val media = HistoryMediaStore(paths.mediaDir, fileIdentity, mediaWriter, fileIo)

    constructor(context: Context, json: Json, readOnlySnapshots: Boolean = false) :
        this(buildPaths(context.applicationContext), json, readOnlySnapshots = readOnlySnapshots)

    override fun paths(): HistoryPaths = paths

    override fun loadDatabase(): StoredHistoryDatabase {
        val database = atomicFiles.read(paths.databaseFile)?.let {
            json.decodeFromString<StoredHistoryDatabase>(it)
        } ?: StoredHistoryDatabase()
        require(database.version == 1) { "Unsupported history database version" }
        return database
    }

    override fun loadSettings(): HistorySettings {
        val settings = atomicFiles.read(paths.settingsFile)?.let {
            json.decodeFromString<HistorySettings>(it)
        } ?: HistorySettings()
        require(settings.schemaVersion == 1) { "Unsupported history settings version" }
        return normalizeHistorySettings(settings)
    }

    override fun saveDatabase(database: StoredHistoryDatabase): HistoryWriteResult =
        atomicFiles.write(paths.databaseFile, json.encodeToString(StoredHistoryDatabase.serializer(), database))

    override fun saveSettings(settings: HistorySettings): HistoryWriteResult =
        atomicFiles.write(paths.settingsFile, json.encodeToString(HistorySettings.serializer(), settings))

    override fun createMedia(type: HistoryType, bytes: ByteArray): HistoryPreparedMedia =
        media.create(type, bytes)

    override fun removeMedia(item: HistoryItem) = media.remove(item)

    override fun mediaFile(item: HistoryItem): File? = historyMediaFile(paths.mediaDir, item, fileIdentity)

    override fun mediaProofForOpen(item: HistoryItem): HistoryMediaProof? =
        observeHistoryMedia(fileIo, paths.mediaDir.toPath(), item)

    override fun close() = atomicFiles.close()

    private companion object {
        fun buildPaths(context: Context): HistoryPaths {
            val externalRoot = context.getExternalFilesDir(null)?.resolve("history")
            val rootDir = externalRoot ?: File(context.filesDir, "history")
            val mediaDir = rootDir.resolve("history_media")
            return HistoryPaths(
                rootDir, rootDir.resolve("history.json"), rootDir.resolve("history_settings.json"), mediaDir,
                externalRoot != null && mediaDir.absolutePath.startsWith("/storage/emulated/0/"),
            )
        }
    }
}
