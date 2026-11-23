package dev.screengoated.toolbox.mobile.service

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

internal class StaleTranslationRequestException : Exception()

internal suspend fun ensureTranslationRequestCurrent(isCurrent: () -> Boolean) {
    currentCoroutineContext().ensureActive()
    if (!isCurrent()) throw StaleTranslationRequestException()
}

internal suspend fun <T> withTranslationFallback(
    isCurrent: () -> Boolean,
    primary: suspend () -> T,
    fallback: suspend () -> T,
): T {
    return try {
        primary()
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (stale: StaleTranslationRequestException) {
        throw stale
    } catch (_: Exception) {
        ensureTranslationRequestCurrent(isCurrent)
        fallback()
    }
}
