package dev.screengoated.toolbox.mobile.history

import java.io.File
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.channels.FileChannel
import java.nio.charset.CodingErrorAction
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.StandardOpenOption.READ
import java.nio.file.attribute.BasicFileAttributes
import java.security.MessageDigest
import android.system.Os
import android.system.OsConstants
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AndroidFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AtomicPublicationUnavailable
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileFingerprint
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileLinkOwnershipChanged
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileObjectId
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionFile

internal data class HistoryFileVersion(val identity: String, val size: Long, val digest: String)

internal fun androidHistoryFileIdentity(path: Path): String {
    val stat = Os.lstat(path.toString())
    if (!OsConstants.S_ISREG(stat.st_mode)) throw IOException("History file unavailable")
    return "${stat.st_dev}:${stat.st_ino}"
}

private fun historyHex(bytes: ByteArray): String = buildString(bytes.size * 2) {
    val digits = "0123456789abcdef"
    bytes.forEach { append(digits[(it.toInt() and 255) ushr 4]); append(digits[it.toInt() and 15]) }
}

internal fun historyDigest(bytes: ByteArray): String = historyHex(MessageDigest.getInstance("SHA-256").digest(bytes))

internal fun historyRegularParents(path: Path) {
    var current = path.toAbsolutePath().parent
    while (current != null) {
        val attributes = try {
            Files.readAttributes(current, BasicFileAttributes::class.java, NOFOLLOW_LINKS)
        } catch (_: NoSuchFileException) {
            current = current.parent
            continue
        }
        if (!attributes.isDirectory || attributes.isSymbolicLink) throw IOException("History directory unavailable")
        current = current.parent
    }
}

internal fun historyFileVersion(
    path: Path,
    fileIdentity: (Path) -> String = ::androidHistoryFileIdentity,
): HistoryFileVersion? {
    historyRegularParents(path)
    val before = try {
        Files.readAttributes(path, BasicFileAttributes::class.java, NOFOLLOW_LINKS)
    } catch (_: NoSuchFileException) {
        return null
    }
    if (!before.isRegularFile || before.isSymbolicLink) throw IOException("History file unavailable")
    val identity = fileIdentity(path)
    val digest = MessageDigest.getInstance("SHA-256")
    var size = 0L
    FileChannel.open(path, READ, NOFOLLOW_LINKS).use { channel ->
        val buffer = ByteBuffer.allocate(64 * 1024)
        while (true) {
            val read = channel.read(buffer)
            if (read < 0) break
            size = Math.addExact(size, read.toLong())
            if (size > before.size()) throw IOException("History file changed")
            buffer.flip()
            digest.update(buffer)
            buffer.clear()
        }
    }
    val observedIdentity = fileIdentity(path)
    val after = Files.readAttributes(path, BasicFileAttributes::class.java, NOFOLLOW_LINKS)
    if (observedIdentity != identity || !after.isRegularFile || after.fileKey() != before.fileKey() ||
        after.creationTime() != before.creationTime() || after.size() != before.size() ||
        after.lastModifiedTime() != before.lastModifiedTime() || size != before.size()
    ) throw IOException("History file changed")
    return HistoryFileVersion(identity, size, historyHex(digest.digest()))
}

/** Atomic replacement is mandatory; uncertain publication requires reopening storage. */
internal class HistoryAtomicFiles(
    private val io: FileTransactionIo = AndroidFileTransactionIo,
    private val maximumSnapshotBytes: Int = (Runtime.getRuntime().maxMemory() / 16)
        .coerceIn(1, Int.MAX_VALUE.toLong() - 8).toInt(),
    private val readOnlySnapshots: Boolean = false,
) : AutoCloseable {
    private data class Loaded(val version: HistoryDescriptorVersion?, val ancestors: List<FileObjectId>?, val pin: TransactionFile?)
    private val loaded = mutableMapOf<Path, Loaded>()
    private var closed = false

    init { require(maximumSnapshotBytes > 0) }

    private fun version(route: HistoryDirectoryRoute, name: String) =
        historyDescriptorVersion(io, route, name, maximumSnapshotBytes)

    private fun exists(path: Path): Boolean = try {
        Files.readAttributes(path, BasicFileAttributes::class.java, NOFOLLOW_LINKS)
        true
    } catch (_: NoSuchFileException) { false }

    @Synchronized
    fun read(file: File): String? = mediaOperation {
        check(!closed) { "History storage is closed" }
        val path = file.toPath().toAbsolutePath().normalize()
        historyRegularParents(path)
        if (!exists(path) && !exists(path.resolveSibling(historyReceiptName(path.fileName.toString()))) &&
            !exists(path.resolveSibling(historyStageName(path.fileName.toString())))
        ) {
            // Absence grants no replacement authority: the first write must be no-replace.
            replaceLoaded(path, Loaded(null, null, null))
            return@mediaOperation null
        }
        HistoryDirectoryRoute(io, path.parent).use { route ->
            HistoryReceipt.open(io, route, path.fileName.toString(), maximumSnapshotBytes, create = false).use { receipt ->
                readLocked(route, path, receipt)
            }
        }
    }

    private fun readLocked(route: HistoryDirectoryRoute, path: Path, receipt: HistoryReceipt?): String? {
        val name = path.fileName.toString()
        if (receipt == null && version(route, historyStageName(name)) != null) {
            throw IOException("History staging ownership is unavailable")
        }
        val observed = receipt?.observe(durableRead = true)
        val original = io.file(route.parent, name)
        var retained = false
        try {
            val bytes = original?.readBounded(maximumSnapshotBytes)
            val before = original?.historyVersion(maximumSnapshotBytes)
            if (before != version(route, name) || (receipt != null && before != observed?.target) ||
                (before != null && before.fingerprint != FileFingerprint(checkNotNull(bytes).size.toLong(), historyDigest(bytes)))
            ) throw IOException("History file changed")
            if (receipt != null && receipt.observe() != observed) throw IOException("History ownership state changed")
            if (receipt == null && version(route, historyReceiptName(name)) != null) {
                throw IOException("History ownership state changed")
            }
            route.revalidate()
            val text = bytes?.let {
                Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(it)).toString()
            }
            retained = !readOnlySnapshots && original != null
            replaceLoaded(path, Loaded(before, route.identities, if (retained) original else null))
            return text
        } finally {
            if (!retained) original?.close()
        }
    }

    @Synchronized
    fun write(file: File, text: String): HistoryWriteResult {
        if (closed || readOnlySnapshots) return HistoryWriteResult.UNAVAILABLE
        val path = file.toPath().toAbsolutePath().normalize()
        return try {
            if (!loaded.containsKey(path)) read(file)
            val expected = checkNotNull(loaded[path])
            val bytes = text.toByteArray(Charsets.UTF_8)
            if (bytes.size > maximumSnapshotBytes) return HistoryWriteResult.REJECTED
            mediaOperation {
                HistoryDirectoryRoute(io, path.parent, createMissing = true).use { route ->
                    val name = path.fileName.toString()
                    if ((expected.ancestors != null && expected.ancestors != route.identities) ||
                        version(route, name) != expected.version
                    ) return@mediaOperation HistoryWriteResult.UNAVAILABLE
                    route.syncCreatedDirectories()
                    route.parent.sync()
                    HistoryReceipt.open(io, route, name, maximumSnapshotBytes, create = true, expected = expected.version).use { opened ->
                        writeLocked(route, path, bytes, expected.version, checkNotNull(opened))
                    }
                }
            }
        } catch (_: Exception) { HistoryWriteResult.UNAVAILABLE }
    }

    private fun writeLocked(route: HistoryDirectoryRoute, path: Path, bytes: ByteArray,
                            expected: HistoryDescriptorVersion?, receipt: HistoryReceipt): HistoryWriteResult {
        var attempted = false
        var published = false
        try {
            val view = receipt.observe()
            if (view.target != expected) return HistoryWriteResult.UNAVAILABLE
            val name = path.fileName.toString()
            val candidate = historyStageName(name)
            val desired = FileFingerprint(bytes.size.toLong(), historyDigest(bytes))
            io.file(route.parent, name).use { retainedTarget ->
                if (retainedTarget?.historyVersion(maximumSnapshotBytes) != expected) return HistoryWriteResult.UNAVAILABLE
                if (view.slot == null) receipt.creating(view, desired)
                val opened = if (view.slot == null) io.file(route.parent, candidate, create = true)
                    else io.writableFile(route.parent, candidate)
                opened.use { stage ->
                    if (stage == null) throw IOException("History staging file unavailable")
                    val before = stage.historyVersion(maximumSnapshotBytes)
                    if ((view.slot != null && before != view.slot) ||
                        (view.slot == null && before.fingerprint != FileFingerprint(0, historyDigest(byteArrayOf()))) ||
                        before.identity == expected?.identity || version(route, candidate) != before
                    ) throw IOException("History staging ownership changed")
                    stage.requireSingleLink()
                    receipt.writing(HistoryReceiptView(view.target, before), desired)
                    stage.requireSingleLink()
                    stage.writeSynced(bytes) { false }
                    val staged = stage.historyVersion(maximumSnapshotBytes)
                    if (staged.fingerprint != desired || version(route, candidate) != staged) {
                        throw IOException("History staging file changed")
                    }
                    route.parent.sync()
                    receipt.prepared(HistoryReceiptView(view.target, staged))
                    if (version(route, name) != expected) return HistoryWriteResult.UNAVAILABLE
                    route.revalidate()
                    receipt.validate()
                    attempted = true
                    io.publish(route.parent, candidate, name, exchange = expected != null)
                    published = true
                    route.parent.sync()
                    val after = HistoryReceiptView(version(route, name), version(route, candidate))
                    if (after != HistoryReceiptView(staged, expected)) return HistoryWriteResult.UNAVAILABLE
                    route.revalidate()
                    receipt.settled(after)
                    if (receipt.observe() != after) return HistoryWriteResult.UNAVAILABLE
                    val pin = io.file(route.parent, name) ?: return HistoryWriteResult.UNAVAILABLE
                    var retained = false
                    try {
                        if (pin.historyVersion(maximumSnapshotBytes) != staged || version(route, name) != staged) {
                            return HistoryWriteResult.UNAVAILABLE
                        }
                        route.revalidate()
                        retained = true
                        replaceLoaded(path, Loaded(staged, route.identities, pin))
                    } finally {
                        if (!retained) pin.close()
                    }
                    return HistoryWriteResult.COMMITTED
                }
            }
        } catch (error: Exception) {
            val noEffect = try { receipt.observe().target == expected } catch (_: Exception) { false }
            return if (noEffect && error !is FileLinkOwnershipChanged && ((!attempted && error !is HistoryDirectoryChanged) ||
                (attempted && !published && error is AtomicPublicationUnavailable))
            ) HistoryWriteResult.REJECTED else HistoryWriteResult.UNAVAILABLE
        }
    }

    private fun replaceLoaded(path: Path, next: Loaded) {
        loaded.put(path, next)?.pin?.close()
    }

    @Synchronized
    override fun close() {
        if (closed) return
        closed = true
        var failure: Throwable? = null
        loaded.values.forEach { value ->
            try { value.pin?.close() } catch (error: Throwable) {
                if (failure == null) failure = error else failure.addSuppressed(error)
            }
        }
        loaded.clear()
        failure?.let { throw it }
    }
}
