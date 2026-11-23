package dev.screengoated.toolbox.mobile.creation

import kotlinx.coroutines.CancellationException

internal suspend fun saveCreationSvgDraft(
    serialize: suspend () -> String,
    persist: suspend (String) -> Unit,
    reportFailure: (Throwable) -> Unit,
) {
    try {
        val updated = serialize()
        check(updated.isNotBlank()) { "Vector result could not be serialized" }
        persist(updated)
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (error: Exception) {
        reportFailure(error)
    }
}
