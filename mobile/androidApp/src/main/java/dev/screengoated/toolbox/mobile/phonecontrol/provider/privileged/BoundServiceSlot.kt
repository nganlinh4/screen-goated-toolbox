package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeoutOrNull

internal class ServiceBindTimeoutException(timeoutMs: Long) :
    IllegalStateException("The privileged user service did not connect within $timeoutMs ms.")

/**
 * The connected user service plus at most one in-flight bind. A bind that does
 * not connect in time is forgotten (and its owner told to unbind), so the next
 * caller starts a fresh bind instead of waiting on a dead one forever.
 */
internal class BoundServiceSlot<T : Any>(
    private val isAlive: (T) -> Boolean,
    private val invalidBinderMessage: String,
) {
    private val lock = Any()

    @Volatile
    private var service: T? = null
    private var pending: CompletableDeferred<T>? = null

    fun connected(value: T?) {
        synchronized(lock) {
            service = value
            val waiting = pending
            pending = null
            if (value != null) {
                waiting?.complete(value)
            } else {
                waiting?.completeExceptionally(IllegalStateException(invalidBinderMessage))
            }
        }
    }

    fun invalidate() {
        synchronized(lock) { service = null }
    }

    fun close() {
        synchronized(lock) {
            service = null
            pending?.cancel()
            pending = null
        }
    }

    suspend fun await(timeoutMs: Long, bind: () -> Unit, onTimeout: () -> Unit): T {
        service?.takeIf(isAlive)?.let { return it }
        val waiting = synchronized(lock) {
            service?.takeIf(isAlive)?.let { return it }
            pending ?: CompletableDeferred<T>().also { created ->
                pending = created
                try {
                    bind()
                } catch (error: Throwable) {
                    pending = null
                    created.completeExceptionally(error)
                }
            }
        }
        withTimeoutOrNull(timeoutMs) { waiting.await() }?.let { return it }
        val abandoned = synchronized(lock) {
            (pending === waiting).also { if (it) pending = null }
        }
        if (abandoned) {
            waiting.cancel()
            onTimeout()
        }
        throw ServiceBindTimeoutException(timeoutMs)
    }
}
