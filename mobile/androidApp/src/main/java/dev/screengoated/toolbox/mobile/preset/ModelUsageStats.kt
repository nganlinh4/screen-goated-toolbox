package dev.screengoated.toolbox.mobile.preset

import java.util.concurrent.ConcurrentHashMap
import okhttp3.Headers

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

    fun updateCerebras(modelFullName: String, headers: Headers) {
        val requestRemaining = headers["x-ratelimit-remaining-requests-day"] ?: "?"
        val requestLimit = headers["x-ratelimit-limit-requests-day"] ?: "?"
        val tokenRemaining = headers["x-ratelimit-remaining-tokens-minute"]
        val tokenLimit = headers["x-ratelimit-limit-tokens-minute"]
        val reset = headers["x-ratelimit-reset-tokens-minute"]
        val remaining = buildString {
            append("day ").append(requestRemaining)
            if (tokenRemaining != null) append(" · TPM ").append(tokenRemaining)
            if (reset != null) append(" · reset ").append(reset)
        }
        val total = buildString {
            append(requestLimit)
            if (tokenLimit != null) append(" · ").append(tokenLimit)
        }
        update(modelFullName, remaining, total)
    }

    /** Get all recorded usage entries (model full name → entry). */
    fun getAll(): Map<String, UsageEntry> = stats.toMap()

    /** Get usage for a specific model. */
    fun get(modelFullName: String): UsageEntry? = stats[modelFullName]

    fun clear() = stats.clear()
}
