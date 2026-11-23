package dev.screengoated.toolbox.mobile.preset

enum class AudioPresetLaunchKind {
    CAPTURE,
    INPUT_DICTATION,
}

data class AudioPresetLaunchRequest(
    val presetId: String,
    val kind: AudioPresetLaunchKind,
    val ownerId: String,
    val inputWindowId: String? = null,
    val requestId: String = java.util.UUID.randomUUID().toString(),
)

class AudioPresetLaunchStore {
    private var pendingRequest: AudioPresetLaunchRequest? = null
    private var ownerId: String? = null
    private var resumeOwner: ((String) -> Unit)? = null

    @Synchronized
    fun registerOwner(ownerId: String, resume: (String) -> Unit) {
        pendingRequest = pendingRequest?.takeIf { it.ownerId == ownerId }
        this.ownerId = ownerId
        resumeOwner = resume
    }

    @Synchronized
    fun releaseOwner(ownerId: String) {
        if (this.ownerId == ownerId) {
            this.ownerId = null
            resumeOwner = null
        }
        clearOwner(ownerId)
    }

    @Synchronized
    fun stopCurrentOwnerThen(stop: () -> Unit) {
        ownerId = null
        resumeOwner = null
        pendingRequest = null
        stop()
    }

    @Synchronized
    fun set(request: AudioPresetLaunchRequest): Boolean {
        if (ownerId != request.ownerId) return false
        pendingRequest = request
        return true
    }

    @Synchronized
    fun resumeIfOwned(requestId: String) {
        val request = pendingRequest?.takeIf { it.requestId == requestId && it.ownerId == ownerId } ?: return
        resumeOwner?.invoke(request.requestId)
    }

    @Synchronized
    fun peek(): AudioPresetLaunchRequest? = pendingRequest

    @Synchronized
    fun takeIfId(requestId: String, ownerId: String): AudioPresetLaunchRequest? {
        val request = pendingRequest?.takeIf { it.requestId == requestId && it.ownerId == ownerId } ?: return null
        pendingRequest = null
        return request
    }

    @Synchronized
    fun clearIfId(requestId: String) {
        if (pendingRequest?.requestId == requestId) pendingRequest = null
    }

    @Synchronized
    fun clearOwner(ownerId: String) {
        if (pendingRequest?.ownerId == ownerId) pendingRequest = null
    }

    @Synchronized
    fun clearInputWindow(ownerId: String, inputWindowId: String) {
        if (pendingRequest?.ownerId == ownerId && pendingRequest?.inputWindowId == inputWindowId) {
            pendingRequest = null
        }
    }
}
