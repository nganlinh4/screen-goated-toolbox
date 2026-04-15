package dev.screengoated.toolbox.mobile

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

class AppToastBus {
    private val mutableMessages = MutableSharedFlow<String>(extraBufferCapacity = 32)

    val messages: SharedFlow<String> = mutableMessages.asSharedFlow()

    fun show(message: String) {
        val trimmed = message.trim()
        if (trimmed.isBlank()) {
            return
        }
        mutableMessages.tryEmit(trimmed)
    }
}
