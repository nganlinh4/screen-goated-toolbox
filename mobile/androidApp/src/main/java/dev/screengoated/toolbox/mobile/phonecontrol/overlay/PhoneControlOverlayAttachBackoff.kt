package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.os.SystemClock

/**
 * Remembers a failed overlay attach so state publishes (several per second
 * while the microphone level moves) do not create and dispose a renderer each
 * time. A retry waits for an exponential backoff; an authority change (a new
 * window host) or the overlay becoming unavailable clears the failure.
 */
internal class PhoneControlOverlayAttachBackoff(
    private val now: () -> Long = SystemClock::elapsedRealtime,
) {
    private var failedOwner: Any? = null
    private var failures = 0
    private var retryAtMs = 0L

    fun mayAttach(owner: Any): Boolean {
        if (failedOwner == null) return true
        if (owner !== failedOwner) {
            reset()
            return true
        }
        return now() >= retryAtMs
    }

    /** Records a failure and returns the delay before the next attempt. */
    fun failed(owner: Any): Long {
        if (owner !== failedOwner) failures = 0
        failedOwner = owner
        failures += 1
        val delayMs = overlayAttachRetryDelayMs(failures)
        retryAtMs = now() + delayMs
        return delayMs
    }

    fun reset() {
        failedOwner = null
        failures = 0
        retryAtMs = 0L
    }
}

internal fun overlayAttachRetryDelayMs(failures: Int): Long =
    (INITIAL_RETRY_MS shl (failures - 1).coerceIn(0, MAX_DOUBLINGS)).coerceAtMost(MAX_RETRY_MS)

private const val INITIAL_RETRY_MS = 1_000L
private const val MAX_RETRY_MS = 30_000L
private const val MAX_DOUBLINGS = 5
