package dev.screengoated.toolbox.mobile.preset

internal enum class AudioPermissionDecision { IGNORE, RESUME_CAPTURE, CLEAR_CAPTURE }

internal fun refreshedAudioPermissionDecision(
    requestId: String?,
    store: AudioPresetLaunchStore,
    granted: Boolean,
    refreshPermissions: () -> Unit,
): AudioPermissionDecision {
    refreshPermissions()
    return audioPermissionDecision(requestId, store.peek(), granted)
}

internal fun audioPermissionDecision(
    requestId: String?,
    pending: AudioPresetLaunchRequest?,
    granted: Boolean,
): AudioPermissionDecision {
    if (requestId == null || pending?.requestId != requestId) {
        return AudioPermissionDecision.IGNORE
    }
    return if (granted) AudioPermissionDecision.RESUME_CAPTURE else AudioPermissionDecision.CLEAR_CAPTURE
}

internal fun audioResumeKind(
    request: AudioPresetLaunchRequest,
    ownerId: String,
    inputWindowId: String?,
): AudioPresetLaunchKind? {
    if (request.ownerId != ownerId) return null
    return when (request.kind) {
        AudioPresetLaunchKind.CAPTURE -> AudioPresetLaunchKind.CAPTURE
        AudioPresetLaunchKind.INPUT_DICTATION ->
            AudioPresetLaunchKind.INPUT_DICTATION.takeIf {
                request.inputWindowId != null && request.inputWindowId == inputWindowId
            }
    }
}
