package dev.screengoated.toolbox.mobile.service.nativelibs

import java.util.concurrent.atomic.AtomicBoolean

/** Coordinates removable runtime payloads with the sessions currently using them. */
internal class RuntimeLeaseRegistry<Key : Any>(
    private val removalReady: (Key) -> Unit,
) {
    private val lock = Any()
    private val leaseCounts = mutableMapOf<Key, Int>()
    private val pendingRemovals = mutableSetOf<Key>()

    fun acquire(keys: Collection<Key>): AutoCloseable? {
        val uniqueKeys = keys.distinct()
        if (uniqueKeys.isEmpty()) return Lease(emptyList(), ::release)
        synchronized(lock) {
            if (uniqueKeys.any(pendingRemovals::contains)) return null
            uniqueKeys.forEach { key ->
                leaseCounts[key] = leaseCounts.getOrDefault(key, 0) + 1
            }
        }
        return Lease(uniqueKeys, ::release)
    }

    fun requestRemoval(key: Key) {
        val ready = synchronized(lock) {
            pendingRemovals += key
            leaseCounts.getOrDefault(key, 0) == 0
        }
        if (ready) removalReady(key)
    }

    fun cancelRemoval(key: Key) {
        synchronized(lock) { pendingRemovals -= key }
    }

    fun completeRemoval(key: Key) {
        synchronized(lock) {
            check(leaseCounts.getOrDefault(key, 0) == 0) {
                "Cannot complete removal while a runtime lease is active"
            }
            pendingRemovals -= key
        }
    }

    fun isRemovalPending(key: Key): Boolean = synchronized(lock) {
        key in pendingRemovals
    }

    fun isInUse(key: Key): Boolean = synchronized(lock) {
        leaseCounts.getOrDefault(key, 0) > 0
    }

    private fun release(keys: List<Key>) {
        val ready = synchronized(lock) {
            keys.forEach { key ->
                val remaining = leaseCounts.getOrDefault(key, 0) - 1
                check(remaining >= 0) { "Runtime lease released without an acquisition" }
                if (remaining == 0) leaseCounts -= key else leaseCounts[key] = remaining
            }
            keys.filter { key ->
                key in pendingRemovals && leaseCounts.getOrDefault(key, 0) == 0
            }
        }
        ready.forEach(removalReady)
    }

    private class Lease<Key : Any>(
        private val keys: List<Key>,
        private val releaseKeys: (List<Key>) -> Unit,
    ) : AutoCloseable {
        private val closed = AtomicBoolean(false)

        override fun close() {
            if (closed.compareAndSet(false, true)) releaseKeys(keys)
        }
    }
}
