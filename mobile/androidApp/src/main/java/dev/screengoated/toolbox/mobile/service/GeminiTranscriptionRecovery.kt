package dev.screengoated.toolbox.mobile.service

/** Canonical transcription response guard; clocks and transport stay in the adapter. */
internal class GeminiTranscriptionRecovery {
    private var activeMs = 0L
    private var boundaryMs: Long? = null

    fun audioSent(durationMs: Long) {
        require(durationMs >= 0)
        activeMs += durationMs.coerceAtMost(Long.MAX_VALUE - activeMs)
    }

    fun inputFlushed(nowMs: Long) {
        if (activeMs >= 4_000 && boundaryMs == null) boundaryMs = nowMs
    }

    fun reset() {
        activeMs = 0
        boundaryMs = null
    }

    fun shouldReconnect(nowMs: Long): Boolean =
        boundaryMs?.let { nowMs >= it && nowMs - it >= 8_000 } ?: false
}
