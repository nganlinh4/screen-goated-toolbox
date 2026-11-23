package dev.screengoated.toolbox.mobile.storage

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Retains a failed secure-store open without creating a replacement store. */
internal class SecureBootstrap<T : Any>(private val open: () -> T) {
    private val mutableState = MutableStateFlow<T?>(null)
    val state = mutableState.asStateFlow()
    val current: T? get() = state.value

    @Synchronized
    fun retry(): T? {
        current?.let { return it }
        val opened = try {
            open()
        } catch (_: SecureStoreUnavailableException) {
            return null
        }
        mutableState.value = opened
        return opened
    }
}
