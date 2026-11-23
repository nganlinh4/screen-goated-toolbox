package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileFingerprint
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import java.nio.file.Path
import java.util.UUID

/** Move the name first so only the verified object in the private slot can be deleted. */
internal fun retireHistoryMedia(io: FileTransactionIo, directory: Path, item: HistoryItem) {
    val proof = item.mediaProof ?: return
    if (!historyMediaNameMatches(item.itemType, item.mediaPath)) return
    val fingerprint = FileFingerprint(proof.size, proof.sha256)
    val maximum = proof.size.takeIf { it in 0..Int.MAX_VALUE.toLong() }?.toInt() ?: return
    val stagingDirectory = directory.toAbsolutePath().normalize().parent?.resolve(".history-retired") ?: return
    if (stagingDirectory == directory.toAbsolutePath().normalize()) return

    mediaOperation {
        HistoryDirectoryRoute(io, directory).use { source ->
            val opened = io.file(source.parent, item.mediaPath) ?: return@use
            opened.use ownedFile@ { owned ->
                val identity = owned.identity
                if ("${identity.device}:${identity.inode}" != proof.identity ||
                    owned.fingerprint(maximum) != fingerprint
                ) return@ownedFile
                owned.requireSingleLink()
                HistoryDirectoryRoute(io, stagingDirectory, createMissing = true).use { staging ->
                    staging.syncCreatedDirectories()
                    val slot = UUID.randomUUID().toString()
                    source.revalidate()
                    staging.revalidate()
                    io.moveNoReplace(source.parent, item.mediaPath, staging.parent, slot)
                    var deleted = false
                    try {
                        source.revalidate()
                        openVerifiedHistoryMedia(io, staging, slot, identity, fingerprint, maximum).use { captured ->
                            captured.requireSingleLink()
                            io.unlinkStagedFile(staging.parent, slot, captured)
                            deleted = true
                        }
                        staging.parent.sync()
                        source.parent.sync()
                    } finally {
                        if (!deleted) runCatching {
                            io.moveNoReplace(staging.parent, slot, source.parent, item.mediaPath)
                        }
                    }
                }
            }
        }
    }
}
