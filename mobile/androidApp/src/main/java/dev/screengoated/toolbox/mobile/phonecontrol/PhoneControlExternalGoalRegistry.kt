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
        // The shared limit counts Unicode code points, not UTF-16 units.
        val characters = normalized.codePointCount(0, normalized.length)
        if (normalized.isEmpty() || characters > MAXIMUM_CHARS || pending != null) {
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

    /**
     * Offers the held goal only when the runtime can act on it, and consumes it
     * only once the runtime accepts it. Returns null when nothing was offered.
     */
    fun submitWhen(canAct: Boolean, submit: (String) -> Boolean): PhoneControlExternalGoalSubmission? {
        if (!canAct) return null
        val goal = peek() ?: return null
        val accepted = submit(goal)
        if (accepted) complete(goal)
        return PhoneControlExternalGoalSubmission(accepted, goal.length)
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

internal data class PhoneControlExternalGoalSubmission(val accepted: Boolean, val chars: Int)

internal object PhoneControlExternalGoalRegistry {
    private val slot = PhoneControlExternalGoalSlot()

    fun offer(text: String): Boolean = slot.offer(text)
    fun peek(): String? = slot.peek()
    fun complete(text: String): Boolean = slot.complete(text)
    fun clear() = slot.clear()
    fun submitWhen(canAct: Boolean, submit: (String) -> Boolean) = slot.submitWhen(canAct, submit)
}
