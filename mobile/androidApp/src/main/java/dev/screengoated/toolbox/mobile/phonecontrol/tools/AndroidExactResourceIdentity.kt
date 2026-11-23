package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.system.Os
import android.system.OsConstants
import java.io.File
import java.io.FileDescriptor
import java.security.MessageDigest
import java.nio.file.Files
import java.nio.file.LinkOption
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Linux descriptors retain inode ownership; reads do not exclude unrelated writers. */
internal object AndroidExactResourceIdentitySource : ExactResourceIdentitySource {
    override fun capture(path: String): ExactResourceIdentity {
        require(File(path).isAbsolute && '\u0000' !in path)
        val canonical = File(path).canonicalFile
        check(!Files.isSymbolicLink(canonical.toPath())) { "Resource route is unresolved" }
        var anchor = canonical.parentFile ?: error("Resource has no parent")
        val tail = mutableListOf(canonical.name)
        while (!anchor.exists()) {
            check(!Files.exists(anchor.toPath(), LinkOption.NOFOLLOW_LINKS)) { "Resource ancestor is unresolved" }
            tail.add(0, anchor.name)
            anchor = anchor.parentFile ?: error("Resource has no existing ancestor")
        }
        val parent = Pinned.open(anchor, directory = true)
        var file: Pinned? = null
        return try {
            if (canonical.exists()) file = Pinned.open(canonical, directory = false)
            check(File(path).canonicalFile == canonical) { "Resource route changed" }
            AndroidExactResourceIdentity(path, canonical, parent, tail, file).also {
                check(it.namedIdentityMatches()) { "Resource identity changed during capture" }
            }
        } catch (error: Throwable) {
            file?.close()
            parent.close()
            throw error
        }
    }
}

private data class NativeId(val device: Long, val inode: Long)

private class Pinned(val path: File, val fd: FileDescriptor, val id: NativeId) : AutoCloseable {
    override fun close() { runCatching { Os.close(fd) } }

    companion object {
        fun open(path: File, directory: Boolean): Pinned {
            val flags = OsConstants.O_RDONLY or OsConstants.O_CLOEXEC or OsConstants.O_NOFOLLOW or OsConstants.O_NONBLOCK
            val fd = Os.open(path.path, flags, 0)
            return try {
                val stat = Os.fstat(fd)
                check(if (directory) OsConstants.S_ISDIR(stat.st_mode) else OsConstants.S_ISREG(stat.st_mode))
                Pinned(path, fd, NativeId(stat.st_dev, stat.st_ino))
            } catch (error: Throwable) {
                runCatching { Os.close(fd) }
                throw error
            }
        }
    }
}

private class AndroidExactResourceIdentity(
    private val requested: String,
    private val canonical: File,
    private val parent: Pinned,
    private val tail: List<String>,
    private val file: Pinned?,
) : ExactResourceIdentity {
    override val hasFile: Boolean get() = file != null
    private var closed = false

    override val description: JsonObject get() = buildJsonObject {
        put("canonical_path", canonical.path)
        put("ancestor_device", parent.id.device)
        put("ancestor_inode", parent.id.inode)
        put("entry_tail", tail.joinToString("/"))
        file?.let {
            put("file_device", it.id.device)
            put("file_inode", it.id.inode)
        }
        put("case_alias_policy", "exact_names_only")
    }

    override fun sameEntry(other: ExactResourceIdentity): Boolean =
        other is AndroidExactResourceIdentity && parent.id == other.parent.id && tail == other.tail

    override fun sameFile(other: ExactResourceIdentity): Boolean =
        other is AndroidExactResourceIdentity && file != null && other.file?.id == file.id

    fun namedIdentityMatches(): Boolean = runCatching {
        check(!closed && File(requested).canonicalFile == canonical)
        Pinned.open(parent.path, directory = true).use { check(it.id == parent.id) }
        if (file == null) !canonical.exists() else {
            Pinned.open(canonical, directory = false).use { it.id == file.id }
        }
    }.getOrDefault(false)

    override fun whileReceiptMatches(sha256: String, credit: () -> Unit): Boolean {
        if (sha256.length != 64 || sha256.any { it.digitToIntOrNull(16) == null }) return false
        val retained = file ?: return false
        return runCatching {
            check(namedIdentityMatches())
            val before = Os.fstat(retained.fd)
            check(before.st_size in 0..MAX_BYTES)
            val first = digest(retained.fd)
            val second = digest(retained.fd)
            val after = Os.fstat(retained.fd)
            check(first.equals(sha256, ignoreCase = true) && first == second)
            check(before.st_size == after.st_size && before.st_mtime == after.st_mtime && before.st_ctime == after.st_ctime)
            check(namedIdentityMatches())
            credit()
            true
        }.getOrDefault(false)
    }

    override fun close() {
        if (closed) return
        closed = true
        file?.close()
        parent.close()
    }

    private fun digest(fd: FileDescriptor): String {
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(64 * 1024)
        var offset = 0L
        while (true) {
            val maximum = (MAX_BYTES - offset + 1).coerceAtMost(buffer.size.toLong()).toInt()
            val count = Os.pread(fd, buffer, 0, maximum, offset)
            if (count <= 0) break
            offset += count
            check(offset <= MAX_BYTES)
            digest.update(buffer, 0, count)
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    companion object { const val MAX_BYTES = 32L * 1024 * 1024 }
}
