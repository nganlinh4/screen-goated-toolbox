package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import android.os.ParcelFileDescriptor
import android.system.Os
import android.system.OsConstants
import java.io.ByteArrayOutputStream
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicBoolean

internal object AndroidFileTransactionIo : FileTransactionIo {
    override fun directory(path: String): TransactionDirectory {
        val descriptor = FileNamespaceNative.openDirectory(path.toByteArray(Charsets.UTF_8))
        if (descriptor < 0) throw FileNamespaceException(-descriptor)
        val owned = ParcelFileDescriptor.adoptFd(descriptor)
        return try {
            check(OsConstants.S_ISDIR(Os.fstat(owned.fileDescriptor).st_mode)) { "Target ancestor is not a directory" }
            Directory(owned)
        } catch (error: Throwable) { owned.close(); throw error }
    }

    override fun directory(parent: TransactionDirectory, name: String): TransactionDirectory? {
        val owned = open(parent, name, 1) ?: return null
        return try { Directory(owned) } catch (error: Throwable) { owned.close(); throw error }
    }

    override fun file(parent: TransactionDirectory, name: String, create: Boolean): TransactionFile? =
        openFile(parent, name, if (create) 2 else 0)

    override fun writableFile(parent: TransactionDirectory, name: String): TransactionFile? = openFile(parent, name, 3)

    private fun openFile(parent: TransactionDirectory, name: String, kind: Int): TransactionFile? {
        val owned = open(parent, name, kind) ?: return null
        return try { NativeFile(owned, kind == 0) } catch (error: Throwable) { owned.close(); throw error }
    }

    override fun makeDirectory(parent: TransactionDirectory, name: String): Boolean {
        val error = FileNamespaceNative.makeDirectory(parent.native().owned.fd, name.toByteArray(Charsets.UTF_8))
        if (error == OsConstants.EEXIST) return false
        if (error != 0) throw FileNamespaceException(error)
        return true
    }

    override fun publish(parent: TransactionDirectory, stage: String, target: String, exchange: Boolean) {
        val error = FileNamespaceNative.publish(parent.native().owned.fd,
            stage.toByteArray(Charsets.UTF_8), target.toByteArray(Charsets.UTF_8), exchange)
        if (error == OsConstants.EEXIST && !exchange) throw FileEntryAlreadyExists()
        if (error == OsConstants.ENOSYS || error == OsConstants.EOPNOTSUPP || error == OsConstants.EINVAL) throw AtomicPublicationUnavailable(error)
        if (error != 0) throw FileNamespaceException(error)
    }

    override fun moveNoReplace(source: TransactionDirectory, name: String, target: TransactionDirectory, destination: String) {
        val error = FileNamespaceNative.moveNoReplace(
            source.native().owned.fd, name.toByteArray(Charsets.UTF_8),
            target.native().owned.fd, destination.toByteArray(Charsets.UTF_8),
        )
        if (error == OsConstants.EEXIST) throw FileEntryAlreadyExists()
        if (error != 0) throw FileNamespaceException(error)
    }

    override fun unlinkStagedFile(directory: TransactionDirectory, name: String, expected: TransactionFile) {
        val error = FileNamespaceNative.unlinkStagedFile(
            directory.native().owned.fd, name.toByteArray(Charsets.UTF_8), (expected as NativeFile).owned.fd,
        )
        if (error != 0) throw FileNamespaceException(error)
    }

    private fun open(parent: TransactionDirectory, name: String, kind: Int): ParcelFileDescriptor? {
        val result = FileNamespaceNative.openEntry(parent.native().owned.fd, name.toByteArray(Charsets.UTF_8), kind)
        if (result == -OsConstants.ENOENT && kind != 2) return null
        if (result == -OsConstants.EEXIST && kind == 2) throw FileEntryAlreadyExists()
        if (result < 0) throw FileNamespaceException(-result)
        return ParcelFileDescriptor.adoptFd(result)
    }

    private fun TransactionDirectory.native(): Directory = this as Directory

    private class Directory(val owned: ParcelFileDescriptor) : TransactionDirectory {
        override val identity = owned.identity()
        override fun sync() {
            val error = FileNamespaceNative.syncDirectory(owned.fd)
            if (error != 0) throw FileNamespaceException(error)
        }
        override fun currentPath(): String? = runCatching {
            val path = Os.readlink("/proc/self/fd/${owned.fd}")
            val named = Os.stat(path)
            path.takeIf { FileObjectId(named.st_dev, named.st_ino) == identity && OsConstants.S_ISDIR(named.st_mode) }
        }.getOrNull()
        override fun close() = owned.close()
    }

    private class NativeFile(val owned: ParcelFileDescriptor, private val readOnly: Boolean) : TransactionFile {
        override val identity = owned.identity()

        override fun fingerprint(maximum: Int): FileFingerprint {
            require(maximum >= 0)
            val before = Os.fstat(owned.fileDescriptor)
            if (before.st_size > maximum) throw FileSnapshotLimitExceeded(before.st_size, maximum)
            val first = fingerprintOnce(maximum)
            val second = fingerprintOnce(maximum)
            val after = Os.fstat(owned.fileDescriptor)
            check(first == second && first.bytes == before.st_size && before.st_size == after.st_size &&
                before.st_mtim.tv_sec == after.st_mtim.tv_sec && before.st_mtim.tv_nsec == after.st_mtim.tv_nsec &&
                before.st_ctim.tv_sec == after.st_ctim.tv_sec && before.st_ctim.tv_nsec == after.st_ctim.tv_nsec) {
                "File changed during fingerprint"
            }
            return first
        }

        private fun fingerprintOnce(maximum: Int): FileFingerprint {
            val digest = MessageDigest.getInstance("SHA-256")
            val buffer = ByteArray(64 * 1024)
            var offset = 0L
            while (true) {
                val remaining = maximum.toLong() + 1 - offset
                val count = Os.pread(owned.fileDescriptor, buffer, 0,
                    minOf(remaining, buffer.size.toLong()).toInt(), offset)
                if (count <= 0) break
                offset += count
                if (offset > maximum) throw FileSnapshotLimitExceeded(offset, maximum)
                digest.update(buffer, 0, count)
            }
            return FileFingerprint(offset, digest.digest().joinToString("") { "%02x".format(it) })
        }

        override fun readBounded(maximum: Int): ByteArray = boundedFileSnapshot(maximum, metadata = {
            val stat = Os.fstat(owned.fileDescriptor)
            FileSnapshotMetadata(stat.st_size, stat.st_mtim.tv_sec, stat.st_mtim.tv_nsec,
                stat.st_ctim.tv_sec, stat.st_ctim.tv_nsec)
        }, read = { read(maximum) })

        private fun read(maximum: Int): ByteArray {
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(64 * 1024)
            while (true) {
                val remaining = maximum.toLong() + 1 - output.size()
                val count = Os.pread(owned.fileDescriptor, buffer, 0,
                    minOf(remaining, buffer.size.toLong()).toInt(), output.size().toLong())
                if (count <= 0) return output.toByteArray()
                if (output.size().toLong() + count > maximum) throw FileSnapshotLimitExceeded(output.size().toLong() + count, maximum)
                output.write(buffer, 0, count)
            }
        }

        override fun writeSynced(bytes: ByteArray, cancelled: () -> Boolean) {
            if (cancelled()) throw FileWriteCancelled()
            Os.ftruncate(owned.fileDescriptor, 0)
            Os.lseek(owned.fileDescriptor, 0, OsConstants.SEEK_SET)
            var offset = 0
            while (offset < bytes.size) {
                if (cancelled()) throw FileWriteCancelled()
                val count = Os.write(owned.fileDescriptor, bytes, offset, minOf(64 * 1024, bytes.size - offset))
                check(count > 0) { "File staging made no progress" }
                offset += count
            }
            if (cancelled()) throw FileWriteCancelled()
            Os.fsync(owned.fileDescriptor)
        }

        override fun lockExclusive(): TransactionLock {
            val pin = ParcelFileDescriptor.dup(owned.fileDescriptor)
            try {
                val closed = AtomicBoolean()
                val lease = TransactionLock {
                    if (closed.compareAndSet(false, true)) try {
                        val release = FileNamespaceNative.lockFile(pin.fd, false)
                        if (release != 0) throw FileNamespaceException(release)
                    } finally { pin.close() }
                }
                val error = FileNamespaceNative.lockFile(pin.fd, true)
                if (error != 0) throw FileNamespaceException(error)
                return lease
            } catch (error: Throwable) {
                try { pin.close() } catch (close: Throwable) { error.addSuppressed(close) }
                throw error
            }
        }

        override fun requireSingleLink() {
            val links = Os.fstat(owned.fileDescriptor).st_nlink
            if (links != 1L) throw FileLinkOwnershipChanged(links)
        }

        override fun duplicateReadDescriptor(): ParcelFileDescriptor {
            if (!readOnly) throw java.io.IOException("Descriptor is not read-only")
            return ParcelFileDescriptor.dup(owned.fileDescriptor)
        }

        override fun close() = owned.close()
    }
}

private fun ParcelFileDescriptor.identity(): FileObjectId = Os.fstat(fileDescriptor).let { FileObjectId(it.st_dev, it.st_ino) }
