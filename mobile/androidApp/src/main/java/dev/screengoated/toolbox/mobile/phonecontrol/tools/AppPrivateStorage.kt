package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import java.io.File

/**
 * Storage that belongs to SGT itself: internal data, caches and the
 * app-specific external directories. Phone-control logs, memory and cached
 * artifacts live here, so a model-chosen resource launch must never grant
 * another app read access to it.
 */
internal fun appPrivateStorageRoots(context: Context): List<File> = buildList {
    add(context.dataDir)
    add(context.filesDir)
    add(context.cacheDir)
    add(context.noBackupFilesDir)
    add(context.codeCacheDir)
    context.externalCacheDirs.filterNotNull().forEach(::add)
    context.getExternalFilesDirs(null).filterNotNull().forEach(::add)
}

internal fun File.isWithinAnyRoot(roots: List<File>): Boolean = roots.any { root -> isWithin(root) }

internal fun File.isWithin(root: File): Boolean = runCatching {
    val candidatePath = canonicalFile.toPath()
    val rootPath = root.canonicalFile.toPath()
    candidatePath == rootPath || candidatePath.startsWith(rootPath)
}.getOrDefault(false)

/** True when a content URI authority names one of SGT's own providers. */
internal fun isOwnContentAuthority(authority: String?, packageName: String): Boolean {
    val value = authority?.trim()?.lowercase() ?: return false
    val owner = packageName.lowercase()
    return value == owner || value.startsWith("$owner.")
}
