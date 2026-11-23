package dev.screengoated.toolbox.mobile.history

import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.Path
import java.nio.file.attribute.BasicFileAttributes
import java.nio.channels.FileChannel
import java.nio.ByteBuffer
import java.nio.file.StandardOpenOption.CREATE_NEW
import java.nio.file.StandardOpenOption.WRITE
import kotlinx.serialization.json.Json
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo

// JVM identity stand-in. These tests do not establish Android filesystem identity.
internal fun historyTestFileIdentity(path: Path): String = Files.readAttributes(
    path, BasicFileAttributes::class.java, NOFOLLOW_LINKS,
).creationTime().toString()

internal fun historyDescriptorTestIdentity(io: NioTestFileTransactionIo, path: Path): String {
    val id = io.identityAt(path)
    return "${id.device}:${id.inode}"
}

internal fun historyTestPersistence(paths: HistoryPaths, json: Json): HistoryPersistence {
    val io = NioTestFileTransactionIo()
    val identity: (Path) -> String = { historyDescriptorTestIdentity(io, it) }
    return HistoryPersistence(paths, json, identity, historyTestMediaWriter(identity), io)
}

internal fun historyTestMediaWriter(identity: (Path) -> String = ::historyTestFileIdentity) =
    HistoryMediaWriter { directory, name, bytes ->
        Files.createDirectories(directory)
        val path = directory.resolve(name)
        val channel = FileChannel.open(path, CREATE_NEW, WRITE, NOFOLLOW_LINKS)
        try {
            val proof = HistoryMediaProof(identity(path), bytes.size.toLong(), historyDigest(bytes))
            val buffer = ByteBuffer.wrap(bytes)
            while (buffer.hasRemaining()) channel.write(buffer)
            channel.force(true)
            HistoryPreparedMedia(name, proof, validateOwned = {
                check(channel.isOpen)
                check(historyFileVersion(path, identity) == HistoryFileVersion(proof.identity, proof.size, proof.sha256))
            }, release = { channel.close() })
        } catch (error: Throwable) {
            channel.close()
            throw error
        }
    }
