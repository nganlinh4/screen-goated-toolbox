package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import java.io.File
import java.nio.channels.FileChannel
import java.nio.file.FileAlreadyExistsException
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.StandardOpenOption
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean

/** Host bytes with simulated inode/entry identity and exchange; not native atomicity evidence. */
internal class NioTestFileTransactionIo : FileTransactionIo {
    var beforePublish: () -> Unit = {}
    var afterPublish: () -> Unit = {}
    var beforeMove: () -> Unit = {}
    var afterMove: () -> Unit = {}
    var beforeWrite: () -> Unit = {}
    var afterWrite: () -> Unit = {}
    var afterDirectoryCreate: () -> Unit = {}
    var readFailure: (String) -> Boolean = { false }
    var writeFailureAfter: Int? = null
    var writeFailureAfterForPath: (String) -> Int? = { null }
    var beforeFileWrite: (String, ByteArray) -> Unit = { _, _ -> }
    var beforeDirectorySync: (String) -> Unit = {}
    var fileLinkCount: (String) -> Long = { 1 }

    private inner class Directory(val path: Path) : TransactionDirectory {
        override val identity = identity(path)
        override fun currentPath() = path.toString()
        override fun sync() = beforeDirectorySync(path.toString())
        override fun close() = Unit
    }

    override fun directory(path: String): TransactionDirectory {
        val target = File(path).toPath()
        require(Files.isDirectory(target, LinkOption.NOFOLLOW_LINKS)) { "Not an ordinary directory" }
        return Directory(target)
    }

    override fun directory(parent: TransactionDirectory, name: String): TransactionDirectory? {
        val path = parent.path(name)
        if (!Files.exists(path, LinkOption.NOFOLLOW_LINKS)) return null
        return directory(path.toString())
    }

    override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? {
        val path = parent.path(name)
        if (create) try { Files.createFile(path) } catch (_: FileAlreadyExistsException) { throw FileEntryAlreadyExists() }
        if (create) replaceIdentity(path.toString())
        if (!Files.exists(path, LinkOption.NOFOLLOW_LINKS)) return null
        require(Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS)) { "Not an ordinary file" }
        return object : TransactionFile {
            override val identity = identity(path)
            override fun readBounded(maximum: Int): ByteArray {
                if (readFailure(path.toString())) error("Injected read failure")
                check(identity(path) == identity)
                val length = Files.size(path)
                if (length > maximum) throw FileSnapshotLimitExceeded(length, maximum)
                val bytes = Files.readAllBytes(path)
                check(bytes.size <= maximum && identity(path) == identity)
                return bytes
            }
            override fun writeSynced(bytes: ByteArray, cancelled: () -> Boolean) {
                beforeFileWrite(path.toString(), bytes)
                beforeWrite()
                check(identity(path) == identity)
                if (cancelled()) throw FileWriteCancelled()
                FileChannel.open(path, StandardOpenOption.WRITE).use { output ->
                    output.truncate(0)
                    var offset = 0
                    while (offset < bytes.size) {
                        if (cancelled()) throw FileWriteCancelled()
                        val limit = writeFailureAfterForPath(path.toString()) ?: writeFailureAfter ?: bytes.size
                        if (offset >= limit) error("Injected partial write failure")
                        val count = minOf(8192, bytes.size - offset, limit - offset)
                        val written = output.write(java.nio.ByteBuffer.wrap(bytes, offset, count))
                        check(written > 0)
                        offset += written
                    }
                    output.force(true)
                }
                afterWrite()
            }
            override fun lockExclusive(): TransactionLock {
                synchronized(NAMESPACE_LOCK) { check(locks.add(identity)) { "File lease is busy" } }
                val closed = AtomicBoolean()
                return TransactionLock {
                    if (closed.compareAndSet(false, true)) synchronized(NAMESPACE_LOCK) { check(locks.remove(identity)) }
                }
            }
            override fun requireSingleLink() {
                val links = fileLinkCount(path.toString())
                if (links != 1L) throw FileLinkOwnershipChanged(links)
            }
            override fun close() = Unit
        }
    }

    override fun writableFile(parent: TransactionDirectory, name: String): TransactionFile? = file(parent, name)

    override fun makeDirectory(parent: TransactionDirectory, name: String): Boolean = try {
        Files.createDirectory(parent.path(name)); afterDirectoryCreate(); true
    } catch (_: FileAlreadyExistsException) { false }

    override fun publish(parent: TransactionDirectory, stage: String, target: String, exchange: Boolean) = synchronized(NAMESPACE_LOCK) {
        beforePublish()
        val source = parent.path(stage)
        val destination = parent.path(target)
        val sourceId = identity(source)
        val destinationId = if (exchange) identity(destination) else null
        if (!exchange) {
            try { Files.move(source, destination) } catch (_: FileAlreadyExistsException) { throw FileEntryAlreadyExists() }
        } else {
            if (!Files.exists(destination, LinkOption.NOFOLLOW_LINKS)) throw NoSuchFileException(destination.toString())
            val displaced = parent.path(".test-exchange-${UUID.randomUUID()}")
            Files.move(destination, displaced)
            Files.move(source, destination)
            Files.move(displaced, source)
        }
        synchronized(identities) {
            identities[destination.toAbsolutePath().normalize()] = sourceId
            if (exchange) identities[source.toAbsolutePath().normalize()] = checkNotNull(destinationId)
            else identities.remove(source.toAbsolutePath().normalize())
        }
        afterPublish()
    }

    override fun moveNoReplace(source: TransactionDirectory, name: String, target: TransactionDirectory, destination: String) =
        synchronized(NAMESPACE_LOCK) {
            beforeMove()
            val from = source.path(name)
            val to = target.path(destination)
            val sourceId = identity(from)
            try { Files.move(from, to) } catch (_: FileAlreadyExistsException) { throw FileEntryAlreadyExists() }
            synchronized(identities) {
                identities.remove(from.toAbsolutePath().normalize())
                identities[to.toAbsolutePath().normalize()] = sourceId
            }
            afterMove()
        }

    override fun unlinkStagedFile(directory: TransactionDirectory, name: String, expected: TransactionFile) =
        synchronized(NAMESPACE_LOCK) {
            val path = directory.path(name)
            check(identity(path) == expected.identity)
            Files.delete(path)
            synchronized(identities) { identities.remove(path.toAbsolutePath().normalize()) }
            Unit
        }

    fun identityAt(path: Path): FileObjectId = identity(path)

    fun replaceIdentity(path: String) = synchronized(identities) {
        identities[File(path).toPath().toAbsolutePath().normalize()] = FileObjectId(1, nextIdentity++)
    }

    private fun identity(path: Path): FileObjectId = synchronized(identities) {
        identities.getOrPut(path.toAbsolutePath().normalize()) { FileObjectId(1, nextIdentity++) }
    }

    private fun TransactionDirectory.path(name: String): Path = (this as Directory).path.resolve(name)

    companion object {
        private val NAMESPACE_LOCK = Any()
        private val identities = mutableMapOf<Path, FileObjectId>()
        private var nextIdentity = 1L
        private val locks = mutableSetOf<FileObjectId>()
    }
}
