package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Intent
import java.util.UUID
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** One process-local admission and terminal receipt for a projection start. */
internal enum class PhoneControlProjectionReceipt { SETTLED, RETIRED, EXPIRED }
internal enum class OrphanProjectionReentry { NONE, ACCEPTED, SUPERSEDED, STALE, STALE_UNOWNED }
internal enum class DetachedProjectionReentry { SUPERSEDED, NOTIFIED, RETAINED }

internal sealed interface PhoneControlStopAdmission {
    data object Stale : PhoneControlStopAdmission
    data object Accepted : PhoneControlStopAdmission
    data class Wait(val dispatch: PhoneControlProjectionDispatch) : PhoneControlStopAdmission
}

internal class ProjectionOrphanGeneration {
    private var current: Long? = null
    fun set(generation: Long?) { current = generation }
    fun inspect(generation: Long?): OrphanProjectionReentry {
        if (generation == null) {
            return if (current == null) OrphanProjectionReentry.NONE else {
                current = null
                OrphanProjectionReentry.SUPERSEDED
            }
        }
        if (current == null) return OrphanProjectionReentry.STALE_UNOWNED
        if (current != generation) return OrphanProjectionReentry.STALE
        current = null
        return OrphanProjectionReentry.ACCEPTED
    }
}

internal const val EXTRA_ORPHAN_PROJECTION_REENTRY =
    "dev.screengoated.toolbox.mobile.phonecontrol.ORPHAN_PROJECTION_REENTRY"

internal class LatestProjectionReentry<T> {
    private var value: T? = null

    fun replace(next: T) { value = next }
    fun peek(): T? = value
    fun take(): T? = value.also { value = null }
    fun isEmpty(): Boolean = value == null
}

internal data class PhoneControlProjectionReplay(val intent: Intent?)
internal data class StagedProjectionReentry(val intent: Intent, val sequence: Long)
internal data class StopTeardown(val retryBoundary: String?, val retainedConsent: String?)

internal class PhoneControlProjectionDispatch internal constructor(
    internal val publishOrphanedReentry: (Intent) -> Boolean,
) {
    val id: String = UUID.randomUUID().toString()
    val settled = CompletableDeferred<PhoneControlProjectionReceipt>()
    internal var admitted = false
    internal var expiry: Job? = null
    internal val reentry = LatestProjectionReentry<StagedProjectionReentry>()
    internal val replayGeneration: Long = UUID.randomUUID().leastSignificantBits
}

internal suspend fun awaitSafeProjectionReplay(
    dispatch: PhoneControlProjectionDispatch,
    timeoutMs: Long = 5_000,
): PhoneControlProjectionReceipt {
    withTimeoutOrNull(timeoutMs) { dispatch.settled.await() }?.let { return it }
    if (PhoneControlProjectionDispatches.retireIfPending(dispatch, expired = true)) {
        return PhoneControlProjectionReceipt.EXPIRED
    }
    // An admitted service operation may still have effects; await its receipt.
    return dispatch.settled.await()
}

internal inline fun runProjectionDispatch(id: String?, effect: () -> Unit): Boolean {
    val dispatch = PhoneControlProjectionDispatches.admit(id) ?: return false
    try {
        effect()
    } finally {
        PhoneControlProjectionDispatches.finish(dispatch)
    }
    return true
}

internal object PhoneControlProjectionDispatches {
    private val lock = Any()
    private val pending = mutableMapOf<String, PhoneControlProjectionDispatch>()
    private val orphanGeneration = ProjectionOrphanGeneration()
    private var intentSequence = 0L
    private var detachedReentry: StagedProjectionReentry? = null
    private var projectionGeneration: String = UUID.randomUUID().toString()
    private var reservedConsent: String? = null
    private var stopping = false
    private var waitingStopBoundary: String? = null
    private var teardownComplete = CompletableDeferred<Unit>().apply { complete(Unit) }
    // UI intent admission, service dispatch, and orphan publication share the
    // main thread, so a newer user intent cannot race notification publication.
    private val cleanup by lazy { CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate) }

    fun reserveConsent(beforePublish: (String) -> Unit = {}): String = synchronized(lock) {
        check(pending.isEmpty() && reservedConsent == null && !stopping) {
            "a projection consent, start or stop is already pending"
        }
        val nextGeneration = UUID.randomUUID().toString()
        beforePublish(nextGeneration)
        projectionGeneration = nextGeneration
        reservedConsent = nextGeneration
        nextGeneration
    }

    fun create(
        consentGeneration: String? = null,
        beforePublish: (String) -> Unit = {},
        publishOrphanedReentry: (Intent) -> Boolean = { false },
    ): PhoneControlProjectionDispatch {
        val dispatch = synchronized(lock) {
            check(pending.isEmpty() && !stopping) { "a projection start or stop is already pending" }
            val nextGeneration = consentGeneration ?: UUID.randomUUID().toString()
            check(if (consentGeneration == null) reservedConsent == null else
                consentGeneration == projectionGeneration && consentGeneration == reservedConsent) {
                "the projection consent is no longer current"
            }
            val nextDispatch = PhoneControlProjectionDispatch(publishOrphanedReentry)
            if (consentGeneration == null) {
                beforePublish(nextGeneration)
                projectionGeneration = nextGeneration
            } else {
                reservedConsent = null
            }
            detachedReentry?.takeIf { it.sequence == intentSequence }?.let {
                nextDispatch.reentry.replace(it)
                detachedReentry = null
            }
            nextDispatch.also { pending[it.id] = it }
        }
        dispatch.expiry = cleanup.launch {
            delay(30_000)
            retireIfPending(dispatch, expired = true)
            while (true) {
                delay(30_000)
                if (reapIfIdle(dispatch)) return@launch
            }
        }
        return dispatch
    }

    fun current(): PhoneControlProjectionDispatch? = synchronized(lock) {
        pending.values.singleOrNull()
    }

    fun hasUnsettled(): Boolean = synchronized(lock) {
        pending.values.any { !it.settled.isCompleted }
    }

    fun stopBoundary(): String = synchronized(lock) { projectionGeneration }

    fun publishForCurrentBoundary(boundary: String, sequence: Long? = null, publish: () -> Unit): Boolean = synchronized(lock) {
        if (boundary != projectionGeneration || (sequence != null && sequence != intentSequence)) return@synchronized false
        publish()
        true
    }

    fun ownsConsent(generation: String): Boolean = synchronized(lock) {
        generation == projectionGeneration && generation == reservedConsent && !stopping
    }

    fun releaseConsent(generation: String) = synchronized(lock) {
        if (reservedConsent == generation) reservedConsent = null
    }

    fun inspectOrphanReentry(generation: Long?): OrphanProjectionReentry = synchronized(lock) {
        val result = orphanGeneration.inspect(generation)
        if (result != OrphanProjectionReentry.STALE && result != OrphanProjectionReentry.STALE_UNOWNED) {
            intentSequence = if (intentSequence == Long.MAX_VALUE) 1 else intentSequence + 1
            detachedReentry = null
        }
        result
    }

    fun currentIntentSequence(): Long = synchronized(lock) { intentSequence }

    fun preserveDetachedReentry(
        sequence: Long,
        intent: Intent,
        publish: (Intent) -> Boolean,
    ): DetachedProjectionReentry = synchronized(lock) {
        if (sequence != intentSequence) return@synchronized DetachedProjectionReentry.SUPERSEDED
        val generation = UUID.randomUUID().leastSignificantBits
        val guarded = Intent(intent).putExtra(EXTRA_ORPHAN_PROJECTION_REENTRY, generation)
        if (runCatching { publish(guarded) }.getOrDefault(false)) {
            orphanGeneration.set(generation)
            detachedReentry = null
            DetachedProjectionReentry.NOTIFIED
        } else {
            orphanGeneration.set(null)
            detachedReentry = StagedProjectionReentry(Intent(intent), sequence)
            DetachedProjectionReentry.RETAINED
        }
    }

    fun hasDetachedReentry(): Boolean = synchronized(lock) { detachedReentry != null }

    fun takeDetachedReentry(): Intent? = synchronized(lock) {
        if (stopping || reservedConsent != null || pending.isNotEmpty()) return@synchronized null
        val staged = detachedReentry
        detachedReentry = null
        staged?.takeIf { it.sequence == intentSequence }?.intent
    }

    /** Admit a stop only for the generation observed by its request. */
    fun beginStop(boundary: String?, sequence: Long = currentIntentSequence()): PhoneControlStopAdmission = synchronized(lock) {
        if (boundary == null || boundary != projectionGeneration ||
            (stopping && waitingStopBoundary != boundary)
        ) {
            return@synchronized PhoneControlStopAdmission.Stale
        }
        val dispatch = pending.values.singleOrNull()
        if (dispatch?.admitted == true && !dispatch.settled.isCompleted) {
            beginTeardownLocked()
            waitingStopBoundary = boundary
            return@synchronized PhoneControlStopAdmission.Wait(dispatch)
        }
        beginTeardownLocked()
        waitingStopBoundary = null
        // Any outstanding consent for the stopped generation is now stale.
        projectionGeneration = UUID.randomUUID().toString()
        reservedConsent = null
        val newerAction = sequence != intentSequence
        if (!newerAction) dispatch?.reentry?.take()
        if (dispatch != null && !dispatch.admitted && !dispatch.settled.isCompleted) {
            dispatch.settled.complete(PhoneControlProjectionReceipt.RETIRED)
        }
        if (dispatch != null && dispatch.settled.isCompleted && dispatch.reentry.isEmpty()) {
            pending.remove(dispatch.id)
            dispatch.expiry?.cancel()
        }
        if (!newerAction) {
            orphanGeneration.set(null)
            detachedReentry = null
            intentSequence = if (intentSequence == Long.MAX_VALUE) 1 else intentSequence + 1
        }
        PhoneControlStopAdmission.Accepted
    }

    fun endStop() = synchronized(lock) {
        stopping = false
        waitingStopBoundary = null
        teardownComplete.complete(Unit)
    }

    suspend fun awaitTeardown() {
        while (true) {
            val completion = synchronized(lock) {
                if (!stopping) return
                teardownComplete
            }
            completion.await()
        }
    }

    private fun beginTeardownLocked() {
        if (!stopping) teardownComplete = CompletableDeferred()
        stopping = true
    }

    /** Keep new projection admission closed until the service has actually been destroyed. */
    fun beginTeardown() = synchronized(lock) {
        if (!stopping || waitingStopBoundary != null) {
            // A consent opened before an unexpected shutdown must not become
            // valid again when the teardown barrier is released.
            projectionGeneration = UUID.randomUUID().toString()
        }
        reservedConsent = null
        beginTeardownLocked()
        waitingStopBoundary = null
        val dispatch = pending.values.singleOrNull()
        if (dispatch != null && !dispatch.admitted && !dispatch.settled.isCompleted) {
            // The retiring instance cannot execute a newer queued request.
            dispatch.settled.complete(PhoneControlProjectionReceipt.EXPIRED)
        }
    }

    fun beginTeardownForStop(boundary: String): StopTeardown = synchronized(lock) {
        if (boundary != projectionGeneration && reservedConsent != null && pending.isEmpty()) {
            beginTeardownLocked()
            return@synchronized StopTeardown(null, reservedConsent)
        }
        val ownsStop = boundary == projectionGeneration
        beginTeardown()
        StopTeardown(projectionGeneration.takeIf { ownsStop }, null)
    }

    fun continueTeardown(retainedConsent: String?) = synchronized(lock) {
        if (!stopping || retainedConsent == null || retainedConsent != reservedConsent ||
            retainedConsent != projectionGeneration || pending.isNotEmpty()
        ) beginTeardown()
    }

    internal fun reapIfIdle(dispatch: PhoneControlProjectionDispatch): Boolean = synchronized(lock) {
        if (pending[dispatch.id] !== dispatch) return@synchronized true
        if (!dispatch.settled.isCompleted) return@synchronized false
        val staged = dispatch.reentry.peek()
        val next = staged?.takeIf { it.sequence == intentSequence }?.intent
        if (next != null) {
            val guarded = Intent(next).putExtra(
                EXTRA_ORPHAN_PROJECTION_REENTRY,
                dispatch.replayGeneration,
            )
            if (!runCatching { dispatch.publishOrphanedReentry(guarded) }.getOrDefault(false)) {
                return@synchronized false
            }
            orphanGeneration.set(dispatch.replayGeneration)
            dispatch.reentry.take()
        }
        pending.remove(dispatch.id)
        true
    }

    fun admit(id: String?): PhoneControlProjectionDispatch? = synchronized(lock) {
        val dispatch = pending[id] ?: return@synchronized null
        if (dispatch.admitted || dispatch.settled.isCompleted) return@synchronized null
        dispatch.admitted = true
        dispatch
    }

    /** Retires a queued start only if no service effect has been admitted. */
    fun retireIfPending(
        dispatch: PhoneControlProjectionDispatch,
        expired: Boolean = false,
    ): Boolean = synchronized(lock) {
        if (pending[dispatch.id] !== dispatch || dispatch.admitted || dispatch.settled.isCompleted) {
            return@synchronized false
        }
        dispatch.settled.complete(
            if (expired) PhoneControlProjectionReceipt.EXPIRED else PhoneControlProjectionReceipt.RETIRED,
        )
        true
    }

    fun finish(dispatch: PhoneControlProjectionDispatch) {
        synchronized(lock) {
            if (pending[dispatch.id] !== dispatch) return
            dispatch.settled.complete(PhoneControlProjectionReceipt.SETTLED)
        }
    }

    fun stageReentry(
        dispatch: PhoneControlProjectionDispatch,
        intent: Intent,
        replaceExisting: Boolean = true,
    ): Boolean =
        synchronized(lock) {
            if (pending[dispatch.id] !== dispatch) return@synchronized false
            if (replaceExisting || dispatch.reentry.isEmpty()) {
                dispatch.reentry.replace(StagedProjectionReentry(Intent(intent), intentSequence))
            }
            true
        }

    fun claimReplay(dispatch: PhoneControlProjectionDispatch): PhoneControlProjectionReplay? =
        synchronized(lock) {
            if (pending[dispatch.id] !== dispatch || !dispatch.settled.isCompleted) {
                return@synchronized null
            }
            pending.remove(dispatch.id)
            dispatch.expiry?.cancel()
            val staged = dispatch.reentry.take()
            PhoneControlProjectionReplay(staged?.takeIf { it.sequence == intentSequence }?.intent)
        }

    fun releaseIfNoReentry(dispatch: PhoneControlProjectionDispatch): Boolean = synchronized(lock) {
        if (pending[dispatch.id] !== dispatch || !dispatch.settled.isCompleted ||
            !dispatch.reentry.isEmpty()
        ) return@synchronized false
        pending.remove(dispatch.id)
        dispatch.expiry?.cancel()
        true
    }
}
