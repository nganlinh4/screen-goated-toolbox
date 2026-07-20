package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import java.util.concurrent.atomic.AtomicReference

internal fun debugProbeAllows(tool: String, mutationAcknowledged: Boolean): Boolean {
    return mutationAcknowledged || !debugProbeMutationRequired(tool)
}

internal fun debugProbeMutationRequired(tool: String): Boolean =
    PhoneControlToolRegistry.byName[tool]?.requiresMutationAcknowledgement == true

internal data class DebugProbeLease(
    val requestId: String,
    val operation: DebugProbeOperation,
)

internal class DebugProbeOperation(val requestId: String) {
    private val lock = Any()
    private var cancellationRequested = false
    private var cancellationDispatched = false
    private var suppressReceipt = false
    private var cancelAttachedJob: (() -> Unit)? = null

    fun attachCancellation(cancel: () -> Unit) {
        val dispatchNow = synchronized(lock) {
            check(cancelAttachedJob == null) { "debug probe cancellation already attached" }
            cancelAttachedJob = cancel
            if (cancellationRequested && !cancellationDispatched) {
                cancellationDispatched = true
                true
            } else {
                false
            }
        }
        if (dispatchNow) runCatching(cancel)
    }

    fun requestCancellation(suppressFutureReceipt: Boolean) {
        val cancel = synchronized(lock) {
            cancellationRequested = true
            suppressReceipt = suppressReceipt || suppressFutureReceipt
            cancelAttachedJob?.takeIf { !cancellationDispatched }?.also {
                cancellationDispatched = true
            }
        }
        cancel?.let { runCatching(it) }
    }

    fun publishReceiptIfAllowed(publish: () -> Unit): Boolean = synchronized(lock) {
        if (suppressReceipt) return@synchronized false
        publish()
        true
    }
}

internal class DebugProbeAdmission {
    private val active = AtomicReference<DebugProbeLease?>()

    fun tryAdmit(requestId: String, operation: DebugProbeOperation): DebugProbeLease? {
        require(requestId == operation.requestId) { "debug probe identity mismatch" }
        val lease = DebugProbeLease(requestId, operation)
        return lease.takeIf { active.compareAndSet(null, lease) }
    }

    fun cancel(requestId: String) {
        active.get()?.takeIf { it.requestId == requestId }
            ?.operation?.requestCancellation(suppressFutureReceipt = true)
    }

    fun release(lease: DebugProbeLease) {
        active.compareAndSet(lease, null)
    }

    fun activeRequestId(): String? = active.get()?.requestId
}
