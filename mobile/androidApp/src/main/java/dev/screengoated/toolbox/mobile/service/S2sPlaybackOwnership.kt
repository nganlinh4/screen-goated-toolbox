package dev.screengoated.toolbox.mobile.service

/** Serializes cancellation against admission of each native playback effect. */
internal class S2sPlaybackOwnership {
    private val lock = Any()
    private var current = true

    fun isCurrent(): Boolean = synchronized(lock) { current }
    fun retire() = synchronized(lock) { current = false }
    fun dispatch(action: () -> Unit): Boolean = synchronized(lock) {
        if (!current) false else { action(); true }
    }
}
