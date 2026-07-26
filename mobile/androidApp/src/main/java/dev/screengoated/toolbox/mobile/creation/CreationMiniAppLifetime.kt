package dev.screengoated.toolbox.mobile.creation

internal class CreationMiniAppLifetime {
    private val lock = Any()

    @Volatile
    private var closed = false

    val isClosed: Boolean
        get() = closed

    fun <T : Any> computeIfOpen(action: () -> T): T? = synchronized(lock) {
        if (closed) null else action()
    }

    fun close(action: () -> Unit): Boolean = synchronized(lock) {
        if (closed) {
            false
        } else {
            closed = true
            action()
            true
        }
    }
}
