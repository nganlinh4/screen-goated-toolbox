package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

internal class CreationJobInputCleanupStore(context: Context) {
    private val filesDir = context.filesDir
    private val root = File(filesDir, "creation/job-inputs")
    private val target = File(filesDir, "creation/state/job-input-cleanup.json")
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val lock = Any()

    fun record(inputPaths: Collection<String>) = synchronized(lock) {
        val directories = inputPaths.mapNotNull(::validatedDirectory).map(File::getAbsolutePath)
        if (directories.isEmpty()) return@synchronized
        val records = (read() + directories).distinct()
        require(records.size <= CREATION_JOB_INPUT_CLEANUP_MAXIMUM_RECORDS) {
            CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
        }
        write(records)
    }

    fun drain() = synchronized(lock) {
        val protected = creationJournalProtectedPaths(filesDir).mapNotNull { path ->
            validatedDirectory(path)?.absolutePath
        }.toSet()
        val retained = retainedCreationJobInputCleanupPaths(
            read(),
            protected,
            { File(it).exists() },
        ) { deleteCreationTreeNoFollow(root, File(it)) }
        write(retained)
    }

    private fun read(): List<String> {
        if (!target.exists()) return emptyList()
        val text = requireNotNull(
            readCreationIndexTextBounded(target, CREATION_JOB_INPUT_CLEANUP_INDEX_MAX_BYTES),
        ) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
        val decoded = json.decodeFromString<List<String>>(text)
        require(
            decoded.size <= CREATION_JOB_INPUT_CLEANUP_MAXIMUM_RECORDS &&
                decoded.distinct().size == decoded.size &&
                decoded.all { validatedDirectory(it)?.absolutePath == it },
        ) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
        return decoded
    }

    private fun write(records: List<String>) {
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(records),
            CREATION_JOB_INPUT_CLEANUP_INDEX_MAX_BYTES,
        )
    }

    private fun validatedDirectory(path: String): File? {
        val rootPath = root.toPath().toAbsolutePath().normalize()
        val candidate = runCatching { File(path).toPath().toAbsolutePath().normalize() }.getOrNull()
            ?: return null
        val directory = if (candidate.parent == rootPath) candidate else candidate.parent
        return directory?.takeIf {
            it.parent == rootPath &&
                it.fileName.toString().matches(CREATION_JOB_INPUT_DIRECTORY_NAME)
        }?.toFile()
    }
}

internal fun retainedCreationJobInputCleanupPaths(
    records: List<String>,
    protectedDirectories: Set<String>,
    exists: (String) -> Boolean,
    delete: (String) -> Boolean,
): List<String> = records.filter { path ->
    path in protectedDirectories || exists(path) && !delete(path)
}

internal const val CREATION_JOB_INPUT_CLEANUP_INDEX_MAX_BYTES = 2L * 1024 * 1024
private const val CREATION_JOB_INPUT_CLEANUP_MAXIMUM_RECORDS = 4_096
private val CREATION_JOB_INPUT_DIRECTORY_NAME = Regex("[a-z0-9_-]{1,160}")
