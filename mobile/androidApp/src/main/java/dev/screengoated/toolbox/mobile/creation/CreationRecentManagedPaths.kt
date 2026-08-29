package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.util.concurrent.ConcurrentHashMap

internal class CreationRecentManagedPaths {
    private val paths = ConcurrentHashMap<String, Long>()

    fun remember(file: File) {
        paths[file.absolutePath] = System.currentTimeMillis()
    }

    fun protectedPaths(nowMs: Long = System.currentTimeMillis()): Set<String> {
        val cutoff = nowMs - RECENT_MANAGED_PROTECTION_MS
        paths.entries.removeAll { it.value < cutoff }
        return paths.keys
    }
}

internal const val RECENT_MANAGED_PROTECTION_MS = 2L * 60 * 1_000
