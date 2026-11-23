package dev.screengoated.toolbox.mobile.history

import java.io.IOException

internal class HistoryPreparedMedia(
    val name: String,
    val proof: HistoryMediaProof,
    private val validateOwned: () -> Unit,
    private val release: () -> Unit,
) : AutoCloseable {
    private var closed = false

    @Synchronized
    fun revalidate() {
        if (closed) throw IOException("History media lease has closed")
        mediaOperation { validateOwned() }
    }

    @Synchronized
    override fun close() {
        if (closed) return
        closed = true
        release()
    }
}

internal fun <T> mediaOperation(action: () -> T): T = try {
    action()
} catch (error: IOException) {
    throw error
} catch (error: LinkageError) {
    throw IOException("History descriptor primitives unavailable", error)
} catch (error: Exception) {
    throw IOException("History media could not be verified", error)
}
