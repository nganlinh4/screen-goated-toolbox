package dev.screengoated.toolbox.mobile.capture

/** Restarts the capture source before admitting samples from a new generation. */
internal class CaptureGenerationFence(
    private val generation: () -> Long,
    private val restart: () -> Unit,
) {
    private var sourceGeneration = generation()

    fun prepareRead(): Long {
        val current = generation()
        if (current != sourceGeneration) {
            restart()
            sourceGeneration = current
        }
        return current
    }

    fun acceptsRead(start: Long): Boolean = generation() == start
}
