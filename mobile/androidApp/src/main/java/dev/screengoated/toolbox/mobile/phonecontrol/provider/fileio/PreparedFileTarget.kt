package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.FileMutationTargetLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.sha256
import java.io.File
import java.io.IOException
import java.nio.charset.CodingErrorAction
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class TargetPreparationException(val code: String, message: String) : IOException(message)

/** An immutable observation and retained handles; preparation never changes the filesystem. */
internal class PreparedFileTarget private constructor(
    val requested: String,
    val canonicalPath: String,
    val maximumBytes: Int,
    private val io: FileTransactionIo,
    private val directories: MutableList<DirectoryObservation>,
    private val missingParents: List<String>,
    private val originalFile: TransactionFile?,
    private val baseline: ByteArray?,
) : AutoCloseable {
    private data class DirectoryObservation(val path: String, val retained: TransactionDirectory)
    val name: String = File(canonicalPath).name
    val lease: FileMutationTargetLease = FileMutationTargetLease(
        canonicalPath, originalFile != null, baseline?.sha256(), originalFile?.identity,
        directories.last().path, directories.last().retained.identity, missingParents + name,
    )
    val createdParents = mutableListOf<String>()
    val attemptedParents = mutableListOf<String>()
    private var closed = false
    private var consumed = false

    val parent: TransactionDirectory get() = directories.last().retained
    fun originalBytes(): ByteArray? = baseline?.copyOf()

    fun proposal(): JsonObject = buildJsonObject {
        put("capability_class", "dedicated_local_file_write")
        put("operation", if (lease.existedBefore) "replace_existing_file" else "create_file")
        put("requested_path", requested)
        put("canonical_target", canonicalPath)
        put("target_existed_before", lease.existedBefore)
        lease.expectedSha256?.let { put("target_sha256", it) }
        lease.objectIdentity?.let { put("target_identity", it.description()) }
        put("existing_parents", buildJsonArray {
            directories.forEach { directory -> add(buildJsonObject {
                put("path", directory.path); put("identity", directory.retained.identity.description())
            }) }
        })
        put("missing_parents", buildJsonArray { missingParents.forEach { add(kotlinx.serialization.json.JsonPrimitive(it)) } })
    }

    @Synchronized fun consume(): Boolean {
        if (closed || consumed) return false
        consumed = true
        return true
    }

    fun revalidate() {
        revalidateRoute()
        revalidateVersion()
    }

    fun revalidateRoute() {
        check(!closed) { "Prepared target has closed" }
        check(File(requested).canonicalPath == canonicalPath) { "Authorized target route changed" }
        directories.forEach { directory ->
            io.directory(directory.path).use { check(it.identity == directory.retained.identity) { "Authorized parent identity changed" } }
        }
    }

    private fun revalidateVersion() {
        if (directories.last().path != File(canonicalPath).parent) {
            check(!File(canonicalPath).exists()) { "A destination appeared after preparation" }
            return
        }
        io.file(parent, name).use { current ->
            if (lease.existedBefore) {
                val bytes = current?.readBounded(maximumBytes)
                if (current == null || current.identity != lease.objectIdentity || !baseline!!.contentEquals(bytes)) {
                    throw FileTargetVersionChanged(bytes?.sha256())
                }
            } else if (current != null) throw FileTargetVersionChanged(appeared = true)
        }
    }

    fun materializeParents(effect: PhoneControlEffectOwner.EffectLease, stillAuthorized: () -> Boolean) {
        revalidate()
        for (component in missingParents) {
            val path = File(directories.last().path, component).path
            var authorized = false
            val active = effect.dispatchBooleanIfActive {
                if (!stillAuthorized()) false else {
                    authorized = true
                    attemptedParents += path
                    io.makeDirectory(parent, component).also { if (it) createdParents += path }
                }
            }
            if (active == null) throw FileWriteCancelled()
            if (!authorized) throw FileAuthorizationExpired()
            val child = io.directory(parent, component) ?: error("Created parent is unavailable")
            directories += DirectoryObservation(path, child)
        }
        revalidate()
    }

    fun open(name: String, create: Boolean = false): TransactionFile? = io.file(parent, name, create)
    fun publish(stage: String, exchange: Boolean) = io.publish(parent, stage, name, exchange)
    fun currentPath(leaf: String = name): String? = parent.currentPath()?.let { File(it, leaf).path }

    override fun close() {
        if (closed) return
        closed = true
        runCatching { originalFile?.close() }
        directories.asReversed().forEach { runCatching { it.retained.close() } }
    }

    companion object {
        fun prepare(path: String, maximumBytes: Int, io: FileTransactionIo,
                    requireExisting: Boolean = false, overwrite: Boolean = true): PreparedFileTarget {
            require(maximumBytes > 0)
            val file = File(path)
            if (!file.isAbsolute || '\u0000' in path || file.name.isBlank()) {
                throw TargetPreparationException("invalid_path", "The output must be an absolute file path")
            }
            if (path.toByteArray(Charsets.UTF_8).size > 4096) throw TargetPreparationException("invalid_path", "The path exceeds its native byte budget")
            Charsets.UTF_8.newEncoder().onMalformedInput(CodingErrorAction.REPORT).encode(java.nio.CharBuffer.wrap(path))
            val canonical = file.canonicalFile
            if (canonical.path.toByteArray(Charsets.UTF_8).size > 4096) throw TargetPreparationException("invalid_path", "The resolved path exceeds its native byte budget")
            val parentPath = canonical.parentFile ?: throw TargetPreparationException("invalid_path", "Output has no parent")
            val ancestors = generateSequence(parentPath) { it.parentFile }.toList().asReversed()
            val directories = mutableListOf<DirectoryObservation>()
            val missing = mutableListOf<String>()
            var source: TransactionFile? = null
            try {
                directories += DirectoryObservation(ancestors.first().path, io.directory(ancestors.first().path))
                for (ancestor in ancestors.drop(1)) {
                    if (missing.isNotEmpty()) { missing += ancestor.name; continue }
                    val child = io.directory(directories.last().retained, ancestor.name)
                    if (child == null) missing += ancestor.name else directories += DirectoryObservation(ancestor.path, child)
                }
                if (missing.isEmpty()) source = io.file(directories.last().retained, canonical.name)
                if (source == null && requireExisting) throw TargetPreparationException("path_unavailable", "The target file is unavailable")
                if (source != null && !overwrite) throw TargetPreparationException("path_exists", "The destination already exists")
                val bytes = source?.readBounded(maximumBytes)
                return PreparedFileTarget(path, canonical.path, maximumBytes, io, directories, missing, source, bytes).also { it.revalidate() }
            } catch (error: Throwable) {
                runCatching { source?.close() }
                directories.asReversed().forEach { runCatching { it.retained.close() } }
                throw error
            }
        }
    }
}

internal fun FileObjectId.description(): JsonObject = buildJsonObject { put("device", device); put("inode", inode) }
