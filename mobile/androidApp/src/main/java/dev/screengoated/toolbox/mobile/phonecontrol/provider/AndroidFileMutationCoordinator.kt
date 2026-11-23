package dev.screengoated.toolbox.mobile.phonecontrol.provider

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileObjectId
import java.io.File
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

internal data class FileMutationTargetLease(
    val canonicalPath: String,
    val existedBefore: Boolean,
    val expectedSha256: String?,
    val objectIdentity: FileObjectId?,
    val ancestorPath: String,
    val ancestorIdentity: FileObjectId,
    val entryTail: List<String>,
) {
    init {
        require(canonicalPath.isNotBlank() && ancestorPath.isNotBlank() && entryTail.isNotEmpty())
        require(existedBefore == (expectedSha256 != null))
        require(existedBefore == (objectIdentity != null))
    }
}

/** Cooperating writers share a commit lock; native publication/audits live in FilePublication. */
internal object AndroidFileMutationCoordinator {
    private val pathLocks = Array(64) { ReentrantLock() }

    fun <T> withExclusivePath(file: File, action: () -> T): T =
        pathLocks[Math.floorMod(file.absolutePath.hashCode(), pathLocks.size)].withLock(action)
}

internal const val EXACT_FILE_CHANGED_CODE = "hash_mismatch"
