package dev.screengoated.toolbox.mobile.phonecontrol.authority

/** Opaque ownership token for one platform-reserved user step. */
class PlatformUserStepToken internal constructor(internal val id: Long)

data class PlatformUserStepSnapshot(
    val generation: Long,
    val activeCount: Int,
) {
    val active: Boolean get() = activeCount > 0
}

/**
 * Process-local structural signal that an Android API is awaiting a user-owned step.
 * Callers cannot attach language or intent labels; authority comes only from token lifetime.
 */
object PlatformUserStepSessionRegistry {
    private val lock = Any()
    private val activeTokenIds = mutableSetOf<Long>()
    private var nextTokenId = 0L
    private var generation = 0L

    fun begin(): PlatformUserStepToken = synchronized(lock) {
        val token = PlatformUserStepToken(++nextTokenId)
        check(activeTokenIds.add(token.id))
        generation += 1
        token
    }

    fun end(token: PlatformUserStepToken): Boolean = synchronized(lock) {
        if (!activeTokenIds.remove(token.id)) return@synchronized false
        generation += 1
        true
    }

    fun hasActiveSession(): Boolean = synchronized(lock) { activeTokenIds.isNotEmpty() }

    fun snapshot(): PlatformUserStepSnapshot = synchronized(lock) {
        PlatformUserStepSnapshot(generation = generation, activeCount = activeTokenIds.size)
    }
}
