package dev.screengoated.toolbox.mobile.phonecontrol.memory

import android.system.Os
import android.system.OsConstants
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.io.FileOutputStream
import java.nio.channels.FileChannel
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.security.MessageDigest

internal data class PhoneControlMemoryPaths(
    val root: File,
    val index: File,
    val sessions: File,
    val corrupt: File,
)

internal class PhoneControlMemoryAtomicStore(
    root: File,
    private val json: Json,
) {
    val paths = PhoneControlMemoryPaths(
        root = root,
        index = File(root, "index.json"),
        sessions = File(root, "sessions"),
        corrupt = File(root, "corrupt"),
    )

    init {
        paths.root.mkdirs()
        paths.sessions.mkdirs()
        paths.corrupt.mkdirs()
        syncDirectory(paths.root.parentFile)
        syncDirectory(paths.root)
    }

    fun recoverInterruptedWrites() {
        recoverIndexTemp()
        paths.sessions.listFiles { file -> file.name.endsWith(TEMP_SUFFIX) }
            ?.forEach(::recoverSessionTemp)
    }

    fun readIndex(): PhoneControlMemoryIndex? {
        val decoded = decode(paths.index, PhoneControlMemoryIndex.serializer()) ?: return null
        checkSchema(decoded.schemaVersion, paths.index.name)
        return decoded
    }

    fun writeIndex(index: PhoneControlMemoryIndex) {
        writeAtomic(paths.index, index, PhoneControlMemoryIndex.serializer())
    }

    fun readSession(sessionId: String): PhoneControlMemorySession? {
        val file = sessionFile(sessionId)
        return decodeSession(file, quarantineInvalid = true)
            ?.takeIf { it.sessionId == sessionId }
    }

    fun readAllSessions(): List<PhoneControlMemorySession> {
        return paths.sessions.listFiles { file -> SESSION_FILE.matches(file.name) }
            ?.mapNotNull { file -> decodeSession(file, quarantineInvalid = true) }
            .orEmpty()
    }

    fun writeSession(session: PhoneControlMemorySession) {
        writeAtomic(sessionFile(session.sessionId), session, PhoneControlMemorySession.serializer())
    }

    fun deleteSession(sessionId: String) {
        if (sessionFile(sessionId).delete()) syncDirectory(paths.sessions)
    }

    fun quarantineSession(sessionId: String) {
        quarantine(sessionFile(sessionId))
    }

    internal fun sessionFile(sessionId: String): File {
        return File(paths.sessions, sessionFileName(sessionId))
    }

    private fun recoverIndexTemp() {
        val temp = tempFile(paths.index)
        if (!temp.exists()) return
        val candidate = decode(temp, PhoneControlMemoryIndex.serializer())
        val current = decode(paths.index, PhoneControlMemoryIndex.serializer())
        candidate?.let { checkSchema(it.schemaVersion, temp.name) }
        current?.let { checkSchema(it.schemaVersion, paths.index.name) }
        val shouldPromote = candidate != null &&
            (current == null || candidate.revision >= current.revision)
        if (shouldPromote) {
            moveReplace(temp, paths.index)
            syncDirectory(paths.index.parentFile)
        } else {
            temp.delete()
        }
    }

    private fun recoverSessionTemp(temp: File) {
        val targetName = temp.name.removeSuffix(TEMP_SUFFIX)
        if (!SESSION_FILE.matches(targetName)) {
            temp.delete()
            return
        }
        val target = File(paths.sessions, targetName)
        val decodedCandidate = decode(temp, PhoneControlMemorySession.serializer())
        val decodedCurrent = decode(target, PhoneControlMemorySession.serializer())
        decodedCandidate?.let { checkSchema(it.schemaVersion, temp.name) }
        decodedCurrent?.let { checkSchema(it.schemaVersion, target.name) }
        val candidate = decodedCandidate?.takeIf { isValidSession(it, targetName) }
        val current = decodedCurrent?.takeIf { isValidSession(it, targetName) }
        val shouldPromote = candidate != null &&
            (current == null || candidate.revision >= current.revision)
        if (shouldPromote) {
            moveReplace(temp, target)
            syncDirectory(target.parentFile)
        } else {
            temp.delete()
        }
    }

    private fun decodeSession(
        file: File,
        quarantineInvalid: Boolean,
    ): PhoneControlMemorySession? {
        val decoded = decode(file, PhoneControlMemorySession.serializer()) ?: run {
            if (file.exists() && quarantineInvalid) quarantine(file)
            return null
        }
        checkSchema(decoded.schemaVersion, file.name)
        if (!isValidSession(decoded, file.name)) {
            if (quarantineInvalid) quarantine(file)
            return null
        }
        return decoded
    }

    private fun isValidSession(session: PhoneControlMemorySession, fileName: String): Boolean {
        if (session.sessionId.isBlank() || fileName != sessionFileName(session.sessionId)) return false
        if (session.revision < 0L || session.startedAtEpochMs < 0L) return false
        if (session.finalizedAtEpochMs != null && session.finalizedAtEpochMs < 0L) return false
        if (session.records.map { it.recordId }.toSet().size != session.records.size) return false
        return session.records.withIndex().all { (index, record) ->
            record.schemaVersion == PHONE_CONTROL_MEMORY_SCHEMA_VERSION &&
                record.ordinal == index.toLong() &&
                record.recordId.isNotBlank() &&
                record.turnId.isNotBlank() &&
                record.createdAtEpochMs >= 0L
        }
    }

    private fun quarantine(file: File) {
        if (!file.exists()) return
        val destination = File(paths.corrupt, file.name)
        runCatching {
            moveReplace(file, destination)
            syncDirectory(file.parentFile)
            syncDirectory(destination.parentFile)
        }
    }

    private fun checkSchema(version: Int, fileName: String) {
        if (version != PHONE_CONTROL_MEMORY_SCHEMA_VERSION) {
            throw PhoneControlMemorySchemaException(fileName, version)
        }
    }

    private fun <T> decode(file: File, serializer: KSerializer<T>): T? {
        if (!file.isFile) return null
        val text = runCatching { file.readText(Charsets.UTF_8) }.getOrNull() ?: return null
        val root = runCatching { json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return null
        val version = runCatching { root["schemaVersion"]?.jsonPrimitive?.intOrNull }
            .getOrNull() ?: return null
        checkSchema(version, file.name)
        return runCatching {
            json.decodeFromString(serializer, text)
        }.getOrNull()
    }

    private fun <T> writeAtomic(file: File, value: T, serializer: KSerializer<T>) {
        file.parentFile?.mkdirs()
        val bytes = json.encodeToString(serializer, value).toByteArray(Charsets.UTF_8)
        val temp = tempFile(file)
        FileOutputStream(temp, false).use { output ->
            output.write(bytes)
            output.fd.sync()
        }
        moveReplace(temp, file)
        syncDirectory(file.parentFile)
    }

    private fun moveReplace(source: File, destination: File) {
        destination.parentFile?.mkdirs()
        try {
            Files.move(
                source.toPath(),
                destination.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
        } catch (_: AtomicMoveNotSupportedException) {
            Files.move(
                source.toPath(),
                destination.toPath(),
                StandardCopyOption.REPLACE_EXISTING,
            )
        }
    }

    private fun syncDirectory(directory: File?) {
        if (directory == null) return
        val nioSynced = runCatching {
            FileChannel.open(directory.toPath(), StandardOpenOption.READ).use { it.force(true) }
        }.isSuccess
        if (nioSynced) return
        runCatching {
            val descriptor = Os.open(
                directory.absolutePath,
                OsConstants.O_RDONLY,
                0,
            )
            try {
                Os.fsync(descriptor)
            } finally {
                Os.close(descriptor)
            }
        }
    }

    private companion object {
        private const val TEMP_SUFFIX = ".tmp"
        private val SESSION_FILE = Regex("session_[0-9a-f]{64}\\.json")

        private fun tempFile(file: File): File = File(file.parentFile, file.name + TEMP_SUFFIX)
    }
}

internal class PhoneControlMemorySchemaException(
    fileName: String,
    version: Int,
) : IllegalStateException("Unsupported Phone Control memory schema $version in $fileName")

internal fun sessionFileName(sessionId: String): String {
    val digest = MessageDigest.getInstance("SHA-256")
        .digest(sessionId.toByteArray(Charsets.UTF_8))
        .joinToString("") { byte -> "%02x".format(byte) }
    return "session_$digest.json"
}
