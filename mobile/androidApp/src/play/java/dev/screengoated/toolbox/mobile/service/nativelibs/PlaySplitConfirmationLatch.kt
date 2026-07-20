package dev.screengoated.toolbox.mobile.service.nativelibs

/** Pure per-session latch: duplicate state callbacks cannot reopen one confirmation requirement. */
internal class PlaySplitConfirmationLatch {
    private val pending = mutableSetOf<Int>()
    private val resolved = mutableSetOf<Int>()

    fun request(sessionId: Int): Boolean {
        if (sessionId in resolved) return false
        return pending.add(sessionId)
    }

    fun restorePending(sessionId: Int): Boolean {
        if (sessionId in resolved) return false
        pending.add(sessionId)
        return true
    }

    fun isPending(sessionId: Int): Boolean = sessionId in pending

    fun markAccepted(sessionId: Int) {
        pending.remove(sessionId)
        resolved.add(sessionId)
    }

    fun clearRequirement(sessionId: Int) {
        pending.remove(sessionId)
        resolved.remove(sessionId)
    }
}
