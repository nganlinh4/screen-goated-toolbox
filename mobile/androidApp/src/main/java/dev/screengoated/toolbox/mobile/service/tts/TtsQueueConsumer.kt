package dev.screengoated.toolbox.mobile.service.tts

import java.util.concurrent.BlockingQueue
import kotlin.concurrent.thread

/**
 * Starts a daemon thread that blocks on [queue] with no idle timeout, so an
 * idle consumer never wakes. It stops once its thread is interrupted, either
 * while waiting or by [handle] restoring the interrupt flag.
 */
internal fun <T : Any> startTtsQueueConsumer(
    name: String,
    queue: BlockingQueue<T>,
    handle: (T) -> Unit,
): Thread = thread(name = name, start = true, isDaemon = true) {
    try {
        while (!Thread.currentThread().isInterrupted) {
            handle(queue.take())
        }
    } catch (_: InterruptedException) {
        Thread.currentThread().interrupt()
    }
}
