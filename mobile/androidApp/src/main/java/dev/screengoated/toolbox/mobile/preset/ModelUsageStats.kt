package dev.screengoated.toolbox.mobile.preset

import java.util.concurrent.ConcurrentHashMap

/**
 * Tracks per-model rate limit data from API response headers.
 * Thread-safe — updated from API call threads, read from UI thread.
 */
object ModelUsageStats {
    data class UsageEntry(
        val remaining: String,
        val total: String,
    )

    private val stats = ConcurrentHashMap<String, UsageEntry>()

    /** Called after a successful API response to record rate limit headers. */
    fun update(modelFullName: String, remaining: String?, total: String?) {
        if (remaining == null && total == null) return
        stats[modelFullName] = UsageEntry(
            remaining = remaining ?: "?",
            total = total ?: "?",
        )
    }

    /** Get all recorded usage entries (model full name → entry). */
    fun getAll(): Map<String, UsageEntry> = stats.toMap()

    /** Get usage for a specific model. */
    fun get(modelFullName: String): UsageEntry? = stats[modelFullName]

    fun clear() = stats.clear()
}
