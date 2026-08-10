package dev.screengoated.toolbox.mobile.service.nativelibs

internal enum class DeferredRemovalState {
    MISSING,
    INSTALLED,
    REMOVAL_PENDING,
}

/** A deferred request is not removal evidence; physical absence is. */
internal fun deferredRemovalState(
    installed: Boolean,
    removalRequested: Boolean,
): DeferredRemovalState = when {
    !installed -> DeferredRemovalState.MISSING
    removalRequested -> DeferredRemovalState.REMOVAL_PENDING
    else -> DeferredRemovalState.INSTALLED
}
