package dev.screengoated.toolbox.mobile.service.moonshine

import java.io.File
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

interface ManagedModelFile {
    val name: String
    val byteCount: Long
    val sha256: String
}

internal object ManagedModelIntegrity {
    fun payloadPresent(directory: File, files: List<ManagedModelFile>): Boolean =
        files.all { contract ->
            runCatching {
                val path = File(directory, contract.name).toPath()
                Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS) &&
                    Files.size(path) == contract.byteCount
            }.getOrDefault(false)
        }

    fun verified(directory: File, files: List<ManagedModelFile>): Boolean =
        files.all { contract -> verified(File(directory, contract.name), contract) }

    fun verified(file: File, contract: ManagedModelFile): Boolean = runCatching {
        val path = file.toPath()
        Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS) &&
            Files.size(path) == contract.byteCount &&
            sha256(file).equals(contract.sha256, ignoreCase = true)
    }.getOrDefault(false)

    fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().buffered(128 * 1024).use { input ->
            val buffer = ByteArray(128 * 1024)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }
    }

    fun finalizeVerifiedPart(part: File, target: File, contract: ManagedModelFile) {
        check(verified(part, contract)) { "Downloaded ${contract.name} failed integrity verification" }
        try {
            Files.move(
                part.toPath(),
                target.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
        } catch (_: AtomicMoveNotSupportedException) {
            Files.move(part.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
        }
    }

    fun removeManagedFiles(directory: File, files: List<ManagedModelFile>): Boolean {
        var removed = true
        files.forEach { contract ->
            removed = removeIfPresent(File(directory, contract.name)) && removed
            removed = removeIfPresent(File(directory, "${contract.name}.part")) && removed
        }
        directory.delete()
        return removed && files.none { contract ->
            File(directory, contract.name).exists() || File(directory, "${contract.name}.part").exists()
        }
    }

    private fun removeIfPresent(file: File): Boolean = !file.exists() || file.delete()
}
