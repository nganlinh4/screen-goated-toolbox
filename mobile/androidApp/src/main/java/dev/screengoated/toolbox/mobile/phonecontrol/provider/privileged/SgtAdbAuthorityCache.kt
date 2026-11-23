package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import kotlinx.serialization.json.JsonObject

internal const val SGT_ADB_AUTHORITY_MAX_AGE_MS = 30_000L

/**
 * Remembers one verified `id -u` probe for one ADB transport generation, so
 * back-to-back commands and elevated taps do not pay an extra shell round trip.
 * A reconnect, disconnect, forget, or the age bound forces a fresh check.
 */
internal class SgtAdbAuthorityCache(
    private val maxAgeMs: Long = SGT_ADB_AUTHORITY_MAX_AGE_MS,
) {
    private var entry: Entry? = null

    @Synchronized
    fun current(connectionGeneration: Long, nowMs: Long): JsonObject? {
        val cached = entry ?: return null
        val ageMs = nowMs - cached.verifiedAtMs
        return cached.probe.takeIf {
            cached.connectionGeneration == connectionGeneration && ageMs in 0 until maxAgeMs
        }
    }

    @Synchronized
    fun remember(connectionGeneration: Long, nowMs: Long, probe: JsonObject) {
        entry = Entry(connectionGeneration, nowMs, probe)
    }

    @Synchronized
    fun clear() {
        entry = null
    }

    private data class Entry(
        val connectionGeneration: Long,
        val verifiedAtMs: Long,
        val probe: JsonObject,
    )
}
