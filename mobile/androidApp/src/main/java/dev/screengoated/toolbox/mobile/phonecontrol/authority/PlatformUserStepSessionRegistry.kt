package dev.screengoated.toolbox.mobile.phonecontrol.authority

/** Opaque ownership token for one platform-reserved user step. */
class PlatformUserStepToken internal constructor(internal val id: Long)

data class PlatformUserStepSnapshot(
    val generation: Long,
    val activeCount: Int,
    val expectedPackageNames: Set<String>,
) {
    val active: Boolean get() = activeCount > 0
}

/**
 * Process-local structural signal that an Android API is awaiting a user-owned step.
 * Callers cannot attach language or intent labels; authority comes only from token lifetime.
 */
object PlatformUserStepSessionRegistry {
    private val lock = Any()
    private val activeSessions = mutableMapOf<Long, Set<String>>()
    private var nextTokenId = 0L
    private var generation = 0L

    fun begin(
        expectedPackageNames: Set<String> = emptySet(),
    ): PlatformUserStepToken = synchronized(lock) {
        require(expectedPackageNames.none(String::isBlank))
        val token = PlatformUserStepToken(++nextTokenId)
        check(activeSessions.put(token.id, expectedPackageNames.toSet()) == null)
        generation += 1
        token
    }

    fun end(token: PlatformUserStepToken): Boolean = synchronized(lock) {
        if (activeSessions.remove(token.id) == null) return@synchronized false
        generation += 1
        true
    }

    fun hasActiveSession(): Boolean = synchronized(lock) { activeSessions.isNotEmpty() }

    fun snapshot(): PlatformUserStepSnapshot = synchronized(lock) {
        PlatformUserStepSnapshot(
            generation = generation,
            activeCount = activeSessions.size,
            expectedPackageNames = activeSessions.values.flatten().toSet(),
        )
    }
}
