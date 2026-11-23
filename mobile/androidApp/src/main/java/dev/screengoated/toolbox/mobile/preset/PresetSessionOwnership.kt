package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow

internal fun PresetExecutionState.finishSession(sessionId: String, error: String? = null): PresetExecutionState =
    if (this.sessionId != sessionId || !isExecuting) this
    else copy(isExecuting = false, isComplete = error == null, error = error)

/** Serializes session replacement with synchronous effects from the owning graph. */
internal class PresetSessionOwnership(private val state: MutableStateFlow<PresetExecutionState>) {
    private val lock = Any()
    private var effectSessionId = state.value.sessionId

    fun <T> transition(action: () -> T): T = synchronized(lock, action)

    fun activateEffects(sessionId: String) { effectSessionId = sessionId }

    fun retireEffects() { effectSessionId = null }

    fun effectLease(sessionId: String) = PresetEffectLease { action ->
        synchronized(lock) {
            if (effectSessionId != sessionId) false else { action(); true }
        }
    }

    fun <T> owned(sessionId: String, action: () -> T): T = synchronized(lock) {
        if (!state.value.isExecuting || state.value.sessionId != sessionId) {
            throw CancellationException("Preset session retired")
        }
        action()
    }

    suspend fun ensureActive(sessionId: String) {
        currentCoroutineContext().ensureActive()
        owned(sessionId) { }
    }

    fun <T> ownedProbe(sessionId: String, attempt: PresetModelAttempt, action: () -> T): T = try {
        owned(sessionId, action)
    } catch (cancelled: CancellationException) {
        releasePresetModelProbe(attempt)
        throw cancelled
    }
}

/** Valid through successful completion, until the session is explicitly retired. */
class PresetEffectLease internal constructor(private val runOwned: (() -> Unit) -> Boolean) {
    fun runIfCurrent(action: () -> Unit): Boolean = runOwned(action)
    fun isCurrent(): Boolean = runIfCurrent { }
}
