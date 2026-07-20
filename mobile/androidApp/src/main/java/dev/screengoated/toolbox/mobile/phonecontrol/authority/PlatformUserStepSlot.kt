package dev.screengoated.toolbox.mobile.phonecontrol.authority

/** Owns at most one opaque platform-user-step session across an asynchronous launcher. */
internal class PlatformUserStepSlot {
    private val lock = Any()
    private var token: PlatformUserStepToken? = null

    fun begin(): Boolean = synchronized(lock) {
        if (token != null) return@synchronized false
        token = PlatformUserStepSessionRegistry.begin()
        true
    }

    fun finish(): Boolean {
        val retiring = synchronized(lock) {
            val current = token ?: return@synchronized null
            token = null
            current
        } ?: return false
        return PlatformUserStepSessionRegistry.end(retiring)
    }
}
