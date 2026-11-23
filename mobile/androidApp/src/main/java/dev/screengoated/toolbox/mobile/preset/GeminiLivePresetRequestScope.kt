package dev.screengoated.toolbox.mobile.preset

import okhttp3.WebSocket

internal suspend fun <T> withOwnedGeminiLivePresetSocket(socket: WebSocket, block: suspend () -> T): T {
    try {
        return block()
    } finally {
        socket.cancel()
    }
}
