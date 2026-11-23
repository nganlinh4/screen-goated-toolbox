package dev.screengoated.toolbox.mobile.translationgummy

import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull

/** Bounded, cancellable wait around platform classification, which may block in Binder. */
internal object TranslationGummyClassifier {
    private val worker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(1),
        { task -> Thread(task, "gummy-language-classifier").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )

    suspend fun detect(text: String, detect: (String) -> String, fallback: (String) -> String): String {
        return withTimeoutOrNull(2_000L) {
            suspendCancellableCoroutine { continuation ->
                try {
                    val task = worker.submit {
                        val result = runCatching { detect(text) }.getOrElse { fallback(text) }
                        continuation.resume(result)
                    }
                    continuation.invokeOnCancellation { task.cancel(true) }
                } catch (_: RejectedExecutionException) {
                    continuation.resume(fallback(text))
                }
            }
        } ?: fallback(text)
    }
}
