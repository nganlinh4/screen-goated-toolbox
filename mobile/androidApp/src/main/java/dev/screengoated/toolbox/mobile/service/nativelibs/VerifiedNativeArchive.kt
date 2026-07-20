package dev.screengoated.toolbox.mobile.service.nativelibs

import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.util.UUID
import java.util.zip.ZipFile

internal object VerifiedNativeArchive {
    fun materialize(
        source: InputStream,
        destination: File,
        contract: NativeRuntimeArchive,
        onProgress: (Float) -> Unit = {},
    ) {
        destination.parentFile?.mkdirs()
        val temp = File(destination.parentFile, ".${destination.name}.${UUID.randomUUID()}.part")
        try {
            val digest = MessageDigest.getInstance("SHA-256")
            var written = 0L
            source.use { input ->
                FileOutputStream(temp).use { output ->
                    val buffer = ByteArray(64 * 1024)
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        written += read
                        require(written <= contract.byteCount) {
                            "${contract.fileName} exceeds its byte contract"
                        }
                        digest.update(buffer, 0, read)
                        output.write(buffer, 0, read)
                        onProgress(written.toFloat() / contract.byteCount.toFloat())
                    }
                    output.fd.sync()
                }
            }
            require(written == contract.byteCount) {
                "${contract.fileName} byte count differs: $written"
            }
            require(digest.hex() == contract.sha256) {
                "${contract.fileName} SHA-256 differs"
            }
            atomicReplace(temp, destination)
        } finally {
            temp.delete()
        }
    }

    fun install(archiveFile: File, libDir: File, contract: NativeRuntimeArchive) {
        validateArchiveIdentity(archiveFile, contract)
        require(libDir.mkdirs() || libDir.isDirectory) { "Could not create native library directory" }
        val token = UUID.randomUUID().toString()
        val staged = linkedMapOf<NativeRuntimeEntry, File>()
        try {
            ZipFile(archiveFile).use { zip ->
                val entries = zip.entries().asSequence().toList()
                require(entries.none { it.isDirectory }) {
                    "${contract.fileName} contains directory entries"
                }
                validateArchiveEntryNames(
                    actual = entries.map { it.name },
                    expected = contract.entries.map { it.fileName },
                )
                val contracts = contract.entries.associateBy { it.fileName }
                entries.forEach { zipEntry ->
                    val expected = requireNotNull(contracts[zipEntry.name])
                    require(zipEntry.size == expected.byteCount) {
                        "${contract.fileName}/${zipEntry.name} byte count differs"
                    }
                    val target = exactTarget(libDir, expected.fileName)
                    val temp = File(libDir, ".${target.name}.$token.part")
                    staged[expected] = temp
                    writeVerifiedMember(zip.getInputStream(zipEntry), temp, expected)
                }
            }
            finalizeTransaction(libDir, staged, token)
        } finally {
            staged.values.forEach(File::delete)
        }
    }

    fun isInstalled(libDir: File, contract: NativeRuntimeArchive): Boolean =
        contract.entries.all { entry ->
            val file = runCatching { exactTarget(libDir, entry.fileName) }.getOrNull()
                ?: return@all false
            file.isFile && file.length() == entry.byteCount && sha256(file) == entry.sha256
        }

    fun validateArchiveIdentity(file: File, contract: NativeRuntimeArchive) {
        require(file.isFile) { "Missing native archive: ${contract.fileName}" }
        require(file.length() == contract.byteCount) {
            "${contract.fileName} byte count differs"
        }
        require(sha256(file) == contract.sha256) { "${contract.fileName} SHA-256 differs" }
    }

    fun validateArchiveEntryNames(actual: List<String>, expected: List<String>) {
        actual.forEach(::requireFlatLibraryName)
        require(actual.size == actual.toSet().size) { "Native archive contains duplicate entries" }
        require(actual.toSet() == expected.toSet() && actual.size == expected.size) {
            "Native archive members differ: expected=${expected.toSet()} actual=${actual.toSet()}"
        }
    }

    fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(1024 * 1024)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.hex()
    }

    private fun writeVerifiedMember(
        source: InputStream,
        destination: File,
        contract: NativeRuntimeEntry,
    ) {
        val digest = MessageDigest.getInstance("SHA-256")
        var written = 0L
        source.use { input ->
            FileOutputStream(destination).use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    written += read
                    require(written <= contract.byteCount) {
                        "${contract.fileName} exceeds its byte contract"
                    }
                    digest.update(buffer, 0, read)
                    output.write(buffer, 0, read)
                }
                output.fd.sync()
            }
        }
        require(written == contract.byteCount) {
            "${contract.fileName} byte count differs: $written"
        }
        require(digest.hex() == contract.sha256) { "${contract.fileName} SHA-256 differs" }
    }

    private fun finalizeTransaction(
        libDir: File,
        staged: Map<NativeRuntimeEntry, File>,
        token: String,
    ) {
        val backups = linkedMapOf<File, File>()
        val finalized = mutableListOf<File>()
        try {
            staged.keys.forEach { entry ->
                val target = exactTarget(libDir, entry.fileName)
                if (target.exists()) {
                    val backup = File(libDir, ".${target.name}.$token.backup")
                    atomicReplace(target, backup)
                    backups[target] = backup
                }
            }
            staged.forEach { (entry, temp) ->
                val target = exactTarget(libDir, entry.fileName)
                atomicReplace(temp, target)
                finalized += target
            }
            backups.values.forEach(File::delete)
        } catch (error: Throwable) {
            finalized.forEach(File::delete)
            backups.forEach { (target, backup) ->
                if (backup.exists()) {
                    runCatching { atomicReplace(backup, target) }
                        .onFailure(error::addSuppressed)
                }
            }
            throw error
        }
    }

    private fun exactTarget(libDir: File, fileName: String): File {
        requireFlatLibraryName(fileName)
        val root = libDir.canonicalFile
        val target = File(root, fileName).canonicalFile
        require(target.parentFile == root && target.name == fileName) {
            "Native library target escapes its directory"
        }
        return target
    }

    private fun atomicReplace(source: File, destination: File) {
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
}

private fun MessageDigest.hex(): String =
    digest().joinToString("") { byte -> "%02x".format(byte) }
