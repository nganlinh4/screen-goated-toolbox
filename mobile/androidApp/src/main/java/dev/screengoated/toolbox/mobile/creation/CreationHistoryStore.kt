package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.UUID
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

internal class CreationHistoryStore(
    context: Context,
    private val files: CreationFileStore,
) {
    private val target = File(context.filesDir, "creation/history.json")
    private val lock = Any()
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }

    fun list(tool: CreationTool): List<CreationHistoryEntry> = synchronized(lock) {
        val all = read()
        val existing = all.filter { files.exists(it.outputPath) }
        if (existing.size != all.size) write(existing)
        existing.filter { it.tool == tool.wireName }.sortedByDescending { it.createdAtMs }
    }

    fun record(
        tool: CreationTool,
        sourcePath: String,
        outputPath: String,
        outputName: String,
        metadata: JsonObject,
    ): CreationHistoryEntry = synchronized(lock) {
        val entry = CreationHistoryEntry(
            id = UUID.randomUUID().toString(),
            tool = tool.wireName,
            sourcePath = sourcePath,
            outputPath = outputPath,
            outputName = outputName,
            createdAtMs = System.currentTimeMillis(),
            metadata = metadata,
        )
        val retained = read().filter { files.exists(it.outputPath) }.toMutableList()
        retained.removeAll { it.outputPath == outputPath }
        retained += entry
        write(retained.sortedByDescending { it.createdAtMs }.take(MAXIMUM_ENTRIES))
        entry
    }

    fun rename(id: String, requestedName: String): CreationHistoryEntry = synchronized(lock) {
        val entries = read().toMutableList()
        val index = entries.indexOfFirst { it.id == id }
        require(index >= 0) { "Saved result is unavailable" }
        val current = entries[index]
        val extension = current.outputName.substringAfterLast('.', "")
        val requestedExtension = requestedName.substringAfterLast('.', "")
        val finalName = if (extension.isNotBlank() && !requestedExtension.equals(extension, true)) {
            "${requestedName.substringBeforeLast('.', requestedName)}.$extension"
        } else {
            requestedName
        }
        val (path, name) = files.rename(current.outputPath, finalName)
        val updated = current.copy(outputPath = path, outputName = name)
        entries[index] = updated
        write(entries)
        updated
    }

    fun delete(id: String): CreationHistoryEntry = synchronized(lock) {
        val entries = read().toMutableList()
        val index = entries.indexOfFirst { it.id == id }
        require(index >= 0) { "Saved result is unavailable" }
        val removed = entries.removeAt(index)
        check(!files.exists(removed.outputPath) || files.delete(removed.outputPath)) {
            "Could not delete result"
        }
        write(entries)
        removed
    }

    private fun read(): List<CreationHistoryEntry> {
        if (!target.isFile) return emptyList()
        return runCatching {
            json.decodeFromString(ListSerializer(CreationHistoryEntry.serializer()), target.readText())
        }.getOrDefault(emptyList())
    }

    private fun write(entries: List<CreationHistoryEntry>) {
        target.parentFile?.mkdirs()
        val temporary = File(target.parentFile, "${target.name}.tmp")
        temporary.writeText(
            json.encodeToString(ListSerializer(CreationHistoryEntry.serializer()), entries),
        )
        runCatching {
            Files.move(
                temporary.toPath(),
                target.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
        }.getOrElse {
            Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
        }
    }

    private companion object {
        const val MAXIMUM_ENTRIES = 200
    }
}
