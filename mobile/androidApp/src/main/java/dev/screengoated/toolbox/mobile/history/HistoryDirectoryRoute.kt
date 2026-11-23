package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TransactionDirectory
import java.io.IOException
import java.nio.file.Path

internal class HistoryDirectoryChanged : IOException("History directory changed")

/** Retain every ancestor, then compare a fresh descriptor walk at each boundary. */
internal class HistoryDirectoryRoute(
    private val io: FileTransactionIo,
    path: Path,
    createMissing: Boolean = false,
) : AutoCloseable {
    private val directory = path.toAbsolutePath().normalize()
    private val root = directory.root ?: throw IOException("History directory root unavailable")
    private val components = directory.map(Path::toString)
    private val retained = mutableListOf<TransactionDirectory>()
    private val createdParents = mutableListOf<TransactionDirectory>()
    val parent: TransactionDirectory get() = retained.last()
    val identities get() = retained.map { it.identity }

    init {
        try {
            retained += io.directory(root.toString())
            for (name in components) {
                val child = io.directory(parent, name) ?: run {
                    if (!createMissing) throw IOException("History directory unavailable")
                    if (io.makeDirectory(parent, name)) createdParents += parent
                    io.directory(parent, name) ?: throw IOException("History directory unavailable")
                }
                retained += child
            }
            revalidate()
        } catch (error: Throwable) {
            close()
            throw error
        }
    }

    fun syncCreatedDirectories() { createdParents.asReversed().forEach { it.sync() } }

    fun revalidate() {
        var current = io.directory(root.toString())
        try {
            if (current.identity != retained.first().identity) throw HistoryDirectoryChanged()
            for ((index, name) in components.withIndex()) {
                val child = io.directory(current, name) ?: throw HistoryDirectoryChanged()
                val previous = current
                current = child
                previous.close()
                if (current.identity != retained[index + 1].identity) throw HistoryDirectoryChanged()
            }
        } finally {
            current.close()
        }
    }

    override fun close() { retained.asReversed().forEach { runCatching { it.close() } } }
}
