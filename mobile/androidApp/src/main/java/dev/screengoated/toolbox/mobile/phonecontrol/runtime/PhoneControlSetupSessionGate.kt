package dev.screengoated.toolbox.mobile.phonecontrol.runtime

/** Owns microphone admission across an app-driven authority setup session. */
internal class PhoneControlSetupSessionGate {
    private val lock = Any()
    private var state = State.NORMAL
    private var freshSessionReady = true
    private var announcementFinished = true

    val inputAdmitted: Boolean
        get() = synchronized(lock) { state == State.NORMAL }

    fun begin(): Boolean = synchronized(lock) {
        if (state == State.ACTIVE) return@synchronized false
        state = State.ACTIVE
        freshSessionReady = false
        announcementFinished = false
        true
    }

    fun finish(waitForAnnouncement: Boolean): Boolean = synchronized(lock) {
        if (state != State.ACTIVE) return@synchronized false
        state = State.AWAITING_CLEAN_SESSION
        freshSessionReady = false
        announcementFinished = !waitForAnnouncement
        true
    }

    fun observeFreshSession(): Boolean = synchronized(lock) {
        if (state != State.AWAITING_CLEAN_SESSION) return@synchronized false
        freshSessionReady = true
        settleIfReady()
    }

    fun observeAnnouncementFinished(): Boolean = synchronized(lock) {
        if (state != State.AWAITING_CLEAN_SESSION) return@synchronized false
        announcementFinished = true
        settleIfReady()
    }

    fun <T> withAdmittedInput(block: () -> T): T? = synchronized(lock) {
        if (state != State.NORMAL) null else block()
    }

    private fun settleIfReady(): Boolean {
        if (!freshSessionReady || !announcementFinished) return false
        state = State.NORMAL
        return true
    }

    private enum class State {
        NORMAL,
        ACTIVE,
        AWAITING_CLEAN_SESSION,
    }
}
