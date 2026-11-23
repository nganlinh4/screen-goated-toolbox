package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AndroidFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import java.io.IOException
import java.nio.file.Path

internal fun interface HistoryMediaWriter {
    fun create(directory: Path, name: String, bytes: ByteArray): HistoryPreparedMedia
}

/** The caller retains the creation descriptor and route through database outcome handling. */
internal class HistoryDescriptorMediaWriter(
    private val io: FileTransactionIo = AndroidFileTransactionIo,
) : HistoryMediaWriter {
    override fun create(directory: Path, name: String, bytes: ByteArray): HistoryPreparedMedia = mediaOperation {
        val route = HistoryDirectoryRoute(io, directory, createMissing = true)
        var retained: TransactionFile? = null
        try {
            route.revalidate()
            route.syncCreatedDirectories()
            val created = io.file(route.parent, name, create = true)
                ?: throw IOException("Created history media unavailable")
            retained = created
            created.writeSynced(bytes) { false }
            val fingerprint = created.fingerprint(bytes.size)
            if (fingerprint.bytes != bytes.size.toLong() || fingerprint.sha256 != historyDigest(bytes)) {
                throw IOException("Created history media changed")
            }
            val proof = HistoryMediaProof("${created.identity.device}:${created.identity.inode}",
                fingerprint.bytes, fingerprint.sha256)
            val prepared = HistoryPreparedMedia(name, proof, validateOwned = {
                route.revalidate()
                if (created.fingerprint(bytes.size) != fingerprint) throw IOException("Created history media changed")
                openVerifiedHistoryMedia(io, route, name, created.identity, fingerprint, bytes.size).use { }
                route.revalidate()
            }, release = {
                try { created.close() } finally { route.close() }
            })
            prepared.revalidate()
            route.parent.sync()
            prepared.revalidate()
            prepared
        } catch (error: Throwable) {
            runCatching { retained?.close() }
            route.close()
            throw error
        }
    }
}

