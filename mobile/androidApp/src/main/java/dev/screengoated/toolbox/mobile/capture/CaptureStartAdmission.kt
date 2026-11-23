package dev.screengoated.toolbox.mobile.capture

/** Releases a prepared capture if its owner retires before device start. */
internal fun startCaptureWithAdmission(
    admit: ((() -> Unit) -> Boolean)?,
    start: () -> Unit,
    release: () -> Unit,
): Boolean {
    val started = try {
        admit?.invoke(start) ?: run { start(); true }
    } catch (error: Throwable) {
        runCatching(release)
        throw error
    }
    if (!started) release()
    return started
}
