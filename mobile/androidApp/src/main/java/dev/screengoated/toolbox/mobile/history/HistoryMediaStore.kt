package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AndroidFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import java.io.File
import java.io.IOException
import java.nio.file.Path
import java.util.UUID

internal fun historyMediaFile(
    directory: File,
    item: HistoryItem,
    fileIdentity: (Path) -> String = ::androidHistoryFileIdentity,
): File? = runCatching {
    val name = item.mediaPath
    if (name.isBlank() || name == "." || name == ".." || name.any { it in "/\\:" || it.code < 32 }) return null
    val path = directory.toPath().toAbsolutePath().normalize().resolve(name)
    if (path.parent != directory.toPath().toAbsolutePath().normalize()) return null
    val version = historyFileVersion(path, fileIdentity) ?: return null
    val proof = item.mediaProof
    if (proof != null && version != HistoryFileVersion(proof.identity, proof.size, proof.sha256)) return null
    path.toFile()
}.getOrNull()

internal class HistoryMediaStore(
    private val directory: File,
    private val fileIdentity: (Path) -> String,
    private val writer: HistoryMediaWriter = HistoryDescriptorMediaWriter(),
    private val fileIo: FileTransactionIo = AndroidFileTransactionIo,
) {
    fun create(type: HistoryType, bytes: ByteArray): HistoryPreparedMedia {
        val extension = when (type) { HistoryType.TEXT -> "txt"; HistoryType.IMAGE -> "png"; HistoryType.AUDIO -> "wav" }
        val path = directory.toPath().resolve("${UUID.randomUUID()}.$extension")
        val prepared = writer.create(path.parent, path.fileName.toString(), bytes)
        try {
            val proof = prepared.proof
            val version = historyFileVersion(path, fileIdentity) ?: throw IOException("History media unavailable")
            if (version != HistoryFileVersion(proof.identity, proof.size, proof.sha256)) {
                throw IOException("History media changed")
            }
            prepared.revalidate()
            return prepared
        } catch (error: Throwable) {
            runCatching { prepared.close() }
            throw error
        }
    }

    fun remove(item: HistoryItem) {
        runCatching { retireHistoryMedia(fileIo, directory.toPath(), item) }
    }
}
