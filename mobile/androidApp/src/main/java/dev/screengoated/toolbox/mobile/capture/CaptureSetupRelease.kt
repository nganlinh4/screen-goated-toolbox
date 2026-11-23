package dev.screengoated.toolbox.mobile.capture

/**
 * Runs capture setup that acquires platform resources before the flow's
 * awaitClose owns them. A setup failure releases them before it is rethrown.
 */
internal fun <T> setUpCaptureOrRelease(release: () -> Unit, setup: () -> T): T =
    try {
        setup()
    } catch (error: Throwable) {
        runCatching(release).exceptionOrNull()?.let { error.addSuppressed(it) }
        throw error
    }
