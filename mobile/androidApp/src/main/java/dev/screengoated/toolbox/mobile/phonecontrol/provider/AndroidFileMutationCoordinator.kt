package dev.screengoated.toolbox.mobile.phonecontrol.provider

import java.io.File
import java.nio.file.FileAlreadyExistsException
import java.nio.file.Files
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

internal sealed interface ExpectedFileCommit {
    data class Replaced(val previousSha256: String) : ExpectedFileCommit

    data class Changed(val actualSha256: String?) : ExpectedFileCommit {
        val code: String = EXACT_FILE_CHANGED_CODE
    }
}

/**
 * Owns the namespace boundary for ordinary-path mutations.
 *
 * Striped locks bound memory while serializing every mutation of the same
 * canonical path inside this process. The filesystem still remains the source
 * of truth, so exact replacement performs a second read at its commit seam.
 */
internal object AndroidFileMutationCoordinator {
    private val pathLocks = Array(PATH_LOCK_STRIPES) { ReentrantLock() }

    fun <T> withExclusivePath(file: File, action: () -> T): T =
        lockFor(file).withLock(action)

    fun createNew(file: File, bytes: ByteArray) {
        Files.write(
            file.toPath(),
            bytes,
            StandardOpenOption.CREATE_NEW,
            StandardOpenOption.WRITE,
        )
    }

    fun stageSibling(file: File, bytes: ByteArray): Path {
        val parent = file.parentFile?.toPath()
            ?: throw NoSuchFileException(file.absolutePath, null, "Destination has no parent directory")
        val staged = Files.createTempFile(parent, ".${file.name}.sgt-", ".tmp")
        return try {
            Files.write(
                staged,
                bytes,
                StandardOpenOption.TRUNCATE_EXISTING,
                StandardOpenOption.WRITE,
            )
            staged
        } catch (error: Throwable) {
            Files.deleteIfExists(staged)
            throw error
        }
    }

    fun replaceIfExpected(
        file: File,
        staged: Path,
        expectedSha256: String,
    ): ExpectedFileCommit {
        val current = try {
            Files.readAllBytes(file.toPath())
        } catch (_: NoSuchFileException) {
            return ExpectedFileCommit.Changed(actualSha256 = null)
        }
        val actualSha256 = current.sha256()
        if (!actualSha256.equals(expectedSha256, ignoreCase = true)) {
            return ExpectedFileCommit.Changed(actualSha256)
        }

        // Keep the validated read directly beside namespace replacement. Any
        // earlier validation is only a preparation check, never commit proof.
        atomicReplace(staged, file.toPath())
        return ExpectedFileCommit.Replaced(actualSha256)
    }

    fun replace(file: File, staged: Path) {
        atomicReplace(staged, file.toPath())
    }

    private fun lockFor(file: File): ReentrantLock {
        val index = Math.floorMod(file.absolutePath.hashCode(), pathLocks.size)
        return pathLocks[index]
    }

    private fun atomicReplace(staged: Path, destination: Path) {
        Files.move(
            staged,
            destination,
            StandardCopyOption.ATOMIC_MOVE,
            StandardCopyOption.REPLACE_EXISTING,
        )
    }
}

internal fun Throwable.isExistingPathConflict(): Boolean = this is FileAlreadyExistsException

private const val PATH_LOCK_STRIPES = 64
internal const val EXACT_FILE_CHANGED_CODE = "hash_mismatch"
