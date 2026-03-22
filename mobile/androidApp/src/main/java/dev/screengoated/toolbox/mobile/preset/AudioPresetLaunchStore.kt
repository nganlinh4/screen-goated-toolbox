package dev.screengoated.toolbox.mobile.preset

enum class AudioPresetLaunchKind {
    CAPTURE,
    REALTIME,
}

data class AudioPresetLaunchRequest(
    val presetId: String,
    val kind: AudioPresetLaunchKind,
)

class AudioPresetLaunchStore {
    @Volatile
    private var pendingRequest: AudioPresetLaunchRequest? = null
    @Volatile
    private var activeRealtimePresetId: String? = null

    fun set(request: AudioPresetLaunchRequest) {
        pendingRequest = request
    }

    fun peek(): AudioPresetLaunchRequest? = pendingRequest

    fun take(): AudioPresetLaunchRequest? {
        val request = pendingRequest
        pendingRequest = null
        return request
    }

    fun clear() {
        pendingRequest = null
    }

    fun setActiveRealtimePresetId(presetId: String?) {
        activeRealtimePresetId = presetId
    }

    fun activeRealtimePresetId(): String? = activeRealtimePresetId
}
