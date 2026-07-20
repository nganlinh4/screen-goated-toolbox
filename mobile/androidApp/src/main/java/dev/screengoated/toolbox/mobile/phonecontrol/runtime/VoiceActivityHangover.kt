package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import java.util.concurrent.atomic.AtomicLong

/** Keeps structural voice activity true across short gaps between speech frames. */
internal class VoiceActivityHangover(
    private val activeThreshold: Float,
    private val hangoverMs: Long,
) {
    private val lastActiveAtMs = AtomicLong(NO_ACTIVITY)

    init {
        require(activeThreshold.isFinite() && activeThreshold in 0f..1f) {
            "activeThreshold must be finite and between 0 and 1"
        }
        require(hangoverMs >= 0L) { "hangoverMs must be non-negative" }
    }

    /** Returns true only when a new above-threshold activity burst begins. */
    fun observe(level: Float, atMs: Long): Boolean {
        require(atMs >= 0L) { "atMs must be non-negative" }
        if (!level.isFinite() || level < activeThreshold) return false
        while (true) {
            val previous = lastActiveAtMs.get()
            val next = maxOf(previous, atMs)
            if (lastActiveAtMs.compareAndSet(previous, next)) {
                return previous == NO_ACTIVITY ||
                    atMs > previous && atMs - previous > hangoverMs
            }
        }
    }

    fun isActive(atMs: Long): Boolean {
        require(atMs >= 0L) { "atMs must be non-negative" }
        val lastActive = lastActiveAtMs.get()
        return lastActive != NO_ACTIVITY &&
            (atMs <= lastActive || atMs - lastActive <= hangoverMs)
    }

    private companion object {
        const val NO_ACTIVITY = -1L
    }
}
