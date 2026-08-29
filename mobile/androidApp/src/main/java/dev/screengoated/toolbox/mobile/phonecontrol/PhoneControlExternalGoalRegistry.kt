package dev.screengoated.toolbox.mobile.phonecontrol

import android.os.SystemClock

internal class PhoneControlExternalGoalSlot(
    private val now: () -> Long = SystemClock::elapsedRealtime,
    private val maximumAgeMs: Long = MAXIMUM_AGE_MS,
) {
    private data class Pending(val text: String, val createdAtMs: Long)

    private var pending: Pending? = null

    @Synchronized
    fun offer(text: String): Boolean {
        expire()
        val normalized = text.trim()
        if (normalized.isEmpty() || normalized.length > MAXIMUM_CHARS || pending != null) {
            return false
        }
        pending = Pending(normalized, now())
        return true
    }

    @Synchronized
    fun peek(): String? {
        expire()
        return pending?.text
    }

    @Synchronized
    fun complete(text: String): Boolean {
        val current = pending ?: return false
        if (current.text != text) return false
        pending = null
        return true
    }

    @Synchronized
    fun clear() {
        pending = null
    }

    private fun expire() {
        val current = pending ?: return
        if (now() - current.createdAtMs > maximumAgeMs) pending = null
    }

    internal companion object {
        const val MAXIMUM_CHARS = 1_024
        const val MAXIMUM_AGE_MS = 120_000L
    }
}

internal object PhoneControlExternalGoalRegistry {
    private val slot = PhoneControlExternalGoalSlot()

    fun offer(text: String): Boolean = slot.offer(text)
    fun peek(): String? = slot.peek()
    fun complete(text: String): Boolean = slot.complete(text)
    fun clear() = slot.clear()
}
