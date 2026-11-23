package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import java.io.IOException
import java.security.MessageDigest
import android.os.ParcelFileDescriptor

internal data class FileObjectId(val device: Long, val inode: Long)
internal data class FileFingerprint(val bytes: Long, val sha256: String)

internal fun interface TransactionLock : AutoCloseable {
    @Throws(IOException::class)
    override fun close()
}

internal interface TransactionDirectory : AutoCloseable {
    val identity: FileObjectId
    fun currentPath(): String?
    fun sync() { throw IOException("Directory durability is unavailable") }
    @Throws(IOException::class)
    override fun close()
}

internal interface TransactionFile : AutoCloseable {
    val identity: FileObjectId
    fun readBounded(maximum: Int): ByteArray
    /** Replace the complete contents through this writable descriptor, then sync. */
    fun writeSynced(bytes: ByteArray, cancelled: () -> Boolean)
    fun lockExclusive(): TransactionLock { throw IOException("File coordination is unavailable") }
    fun requireSingleLink() { throw IOException("File link ownership is unavailable") }
    fun duplicateReadDescriptor(): ParcelFileDescriptor { throw IOException("Descriptor handoff is unavailable") }
    @Throws(IOException::class)
    override fun close()
    fun fingerprint(maximum: Int): FileFingerprint {
        val bytes = readBounded(maximum)
        return FileFingerprint(bytes.size.toLong(), MessageDigest.getInstance("SHA-256")
            .digest(bytes).joinToString("") { "%02x".format(it) })
    }
}

/** Native seams only; transaction planning, validation and receipts have one owner. */
internal interface FileTransactionIo {
    fun directory(path: String): TransactionDirectory
    fun directory(parent: TransactionDirectory, name: String): TransactionDirectory?
    fun file(parent: TransactionDirectory, name: String, create: Boolean = false): TransactionFile?
    fun writableFile(parent: TransactionDirectory, name: String): TransactionFile? {
        throw IOException("Writable file descriptors are unavailable")
    }
    fun makeDirectory(parent: TransactionDirectory, name: String): Boolean
    fun publish(parent: TransactionDirectory, stage: String, target: String, exchange: Boolean)
    fun moveNoReplace(source: TransactionDirectory, name: String, target: TransactionDirectory, destination: String) {
        throw IOException("Cross-directory atomic moves are unavailable")
    }
    fun unlinkStagedFile(directory: TransactionDirectory, name: String, expected: TransactionFile) {
        throw IOException("Staged file removal is unavailable")
    }
}

internal open class FileNamespaceException(val errorNumber: Int) : IOException("Native file namespace operation failed: errno=$errorNumber")
internal class AtomicPublicationUnavailable(errorNumber: Int) : FileNamespaceException(errorNumber)
internal class FileEntryAlreadyExists : IOException("The destination entry already exists")
internal class FileWriteCancelled : IOException("File staging was cancelled")
internal class FileAuthorizationExpired : IOException("The committed request changed before file publication")
internal class FileLinkOwnershipChanged(val links: Long) : IOException("File object has $links namespace links; exactly one is required")
internal class FileSnapshotLimitExceeded(val actualBytes: Long, val maximumBytes: Int) : IOException("File snapshot exceeds its byte budget")
internal class FileTargetVersionChanged(val actualSha256: String? = null, val appeared: Boolean = false) : IOException("The authorized target identity or version changed")
