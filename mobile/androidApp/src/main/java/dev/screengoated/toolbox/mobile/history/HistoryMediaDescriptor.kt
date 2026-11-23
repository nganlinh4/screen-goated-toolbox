package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileFingerprint
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileObjectId
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile
import java.io.IOException
import java.nio.file.Path

internal fun historyMediaLeaf(name: String): Boolean =
    name.isNotBlank() && name != "." && name != ".." && name.none { it in "/\\:" || it.code < 32 }

internal fun historyMediaNameMatches(type: HistoryType, name: String): Boolean {
    if (!historyMediaLeaf(name)) return false
    val expected = when (type) {
        HistoryType.IMAGE -> "png"
        HistoryType.AUDIO -> "wav"
        HistoryType.TEXT -> "txt"
    }
    val dot = name.lastIndexOf('.')
    return dot > 0 && name.substring(dot + 1).equals(expected, ignoreCase = true)
}

internal fun observeHistoryMedia(io: FileTransactionIo, directory: Path, item: HistoryItem): HistoryMediaProof? =
    runCatching {
        mediaOperation {
            if (!historyMediaNameMatches(item.itemType, item.mediaPath)) throw IOException("History media unavailable")
            val expected = item.mediaProof
            val maximum = expected?.size?.takeIf { it in 0..Int.MAX_VALUE.toLong() }?.toInt()
                ?: if (expected == null) Int.MAX_VALUE else throw IOException("History media unavailable")
            HistoryDirectoryRoute(io, directory).use { route ->
                val opened = io.file(route.parent, item.mediaPath) ?: throw IOException("History media unavailable")
                opened.use { file ->
                    val fingerprint = file.fingerprint(maximum)
                    val proof = HistoryMediaProof("${file.identity.device}:${file.identity.inode}", fingerprint.bytes, fingerprint.sha256)
                    if (expected != null && proof != expected) throw IOException("History media changed")
                    route.revalidate()
                    proof
                }
            }
        }
    }.getOrNull()

/** The caller retains the route and the returned descriptor for the entire use. */
internal fun openVerifiedHistoryMedia(
    io: FileTransactionIo,
    route: HistoryDirectoryRoute,
    name: String,
    identity: FileObjectId,
    fingerprint: FileFingerprint,
    maximum: Int,
): TransactionFile = mediaOperation {
    route.revalidate()
    val opened = io.file(route.parent, name) ?: throw IOException("History media unavailable")
    try {
        if (opened.identity != identity || opened.fingerprint(maximum) != fingerprint) {
            throw IOException("History media changed")
        }
        route.revalidate()
        opened
    } catch (error: Throwable) {
        try { opened.close() } catch (close: Throwable) { error.addSuppressed(close) }
        throw error
    }
}
