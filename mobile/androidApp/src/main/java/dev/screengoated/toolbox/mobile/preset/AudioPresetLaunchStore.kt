package dev.screengoated.toolbox.mobile.preset

enum class AudioPresetLaunchKind {
    CAPTURE,
}

data class AudioPresetLaunchRequest(
    val presetId: String,
    val kind: AudioPresetLaunchKind,
)

class AudioPresetLaunchStore {
    @Volatile
    private var pendingRequest: AudioPresetLaunchRequest? = null

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

}
