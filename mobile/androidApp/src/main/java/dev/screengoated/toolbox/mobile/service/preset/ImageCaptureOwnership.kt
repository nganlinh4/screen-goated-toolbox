package dev.screengoated.toolbox.mobile.service.preset

/** Main-thread lifecycle; an outstanding platform screenshot retains its slot until callback. */
internal class ImageCaptureOwnership {
    private var revision = 0L
    private var current: Long? = null
    private var screenshot: Long? = null
    private var showing: Long? = null

    val active: Boolean get() = current != null
    val showInFlight: Boolean get() = showing != null
    val currentRevision: Long get() = revision

    fun start(): Long {
        revision = Math.addExact(revision, 1)
        current = revision
        return revision
    }

    fun stop(): Long {
        revision = Math.addExact(revision, 1)
        current = null
        return revision
    }

    fun retire(owner: Long): Long? = if (isCurrent(owner)) stop() else null
    fun isCurrent(owner: Long): Boolean = current == owner
    fun isRevision(value: Long): Boolean = revision == value
    fun canStart(owner: Long): Boolean = isCurrent(owner) && screenshot == null && showing == null

    fun beginShow(owner: Long): Boolean {
        if (!canStart(owner)) return false
        showing = owner
        return true
    }

    fun finishShow(owner: Long): Boolean {
        if (showing != owner) return false
        showing = null
        return true
    }

    fun beginScreenshot(owner: Long): Boolean {
        if (!canStart(owner)) return false
        screenshot = owner
        return true
    }

    fun finishScreenshot(owner: Long): Boolean {
        if (screenshot != owner) return false
        screenshot = null
        return true
    }
}

/** Restore surfaces only after the exact prior overlay has detached. */
internal fun retireCaptureUi(
    detach: () -> Boolean,
    stillCurrent: () -> Boolean,
    restoreBubble: () -> Unit,
    restoreOverlay: () -> Unit,
): Boolean {
    if (!detach() || !stillCurrent()) return false
    restoreBubble()
    if (!stillCurrent()) return false
    restoreOverlay()
    return stillCurrent()
}

/** A stale close can only retire its exact retained overlay, never deliver old input. */
internal fun retryRetainedCapture(
    owner: Long,
    retainedOwner: Long?,
    showInFlight: Boolean,
    detach: () -> Boolean,
    afterDetached: () -> Unit,
    onFailure: () -> Unit,
): Boolean {
    if (owner != retainedOwner || showInFlight) return false
    if (!detach()) {
        onFailure()
        return false
    }
    afterDetached()
    return true
}
