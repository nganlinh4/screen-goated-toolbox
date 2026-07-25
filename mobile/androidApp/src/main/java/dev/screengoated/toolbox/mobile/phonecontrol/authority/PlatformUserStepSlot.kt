package dev.screengoated.toolbox.mobile.phonecontrol.authority

/** Owns at most one opaque platform-user-step session across an asynchronous launcher. */
internal class PlatformUserStepSlot {
    private val lock = Any()
    private var token: PlatformUserStepToken? = null

    val active: Boolean
        get() = synchronized(lock) { token != null }

    fun begin(expectedPackageName: String? = null): Boolean = synchronized(lock) {
        if (token != null) return@synchronized false
        token = PlatformUserStepSessionRegistry.begin(
            expectedPackageName?.let(::setOf) ?: emptySet(),
        )
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
