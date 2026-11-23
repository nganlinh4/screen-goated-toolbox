package dev.screengoated.toolbox.mobile.creation

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * Process-lifetime IO scope for creation-surface teardown and source cleanup. That
 * work takes the manager's mutation lock and touches disk, so it must never run on
 * the main thread, and it must outlive the ViewModel that requested it.
 */
internal val creationSurfaceIoScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

/** Releases one closed owner in order; every step runs even when an earlier one fails. */
internal fun releaseCreationOwner(
    closeOwner: () -> Unit,
    releaseSources: () -> Unit,
    releaseSurface: () -> Unit,
) {
    try {
        closeOwner()
    } finally {
        try {
            releaseSources()
        } finally {
            releaseSurface()
        }
    }
}
