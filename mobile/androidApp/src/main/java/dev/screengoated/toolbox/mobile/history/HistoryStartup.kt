package dev.screengoated.toolbox.mobile.history

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers

/** Owns the deferred worker and storage until its container is built. */
internal fun <T> withHistoryStartup(
    persistence: HistoryStorage,
    closePersistence: () -> Unit,
    dispatcher: CoroutineDispatcher = Dispatchers.IO,
    onMaxItemsChanged: () -> Unit = {},
    build: (HistoryRepository) -> T,
): T {
    var history: HistoryRepository? = null
    try {
        val repository = HistoryRepository(persistence, dispatcher, deferStartupPrune = true, onMaxItemsChanged)
        history = repository
        val result = build(repository)
        repository.completeDeferredStartup()
        return result
    } catch (error: Throwable) {
        try { history?.cancelDeferredStartup() } catch (cleanup: Throwable) { error.addSuppressed(cleanup) }
        try { closePersistence() } catch (cleanup: Throwable) { error.addSuppressed(cleanup) }
        throw error
    }
}
