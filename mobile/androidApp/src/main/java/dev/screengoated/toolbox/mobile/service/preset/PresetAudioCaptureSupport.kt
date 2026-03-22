package dev.screengoated.toolbox.mobile.service.preset

internal enum class PresetAudioForegroundMode {
    NONE,
    MICROPHONE,
    MEDIA_PROJECTION,
}

internal enum class PresetAudioCaptureFailureReason {
    RECORD_PERMISSION_REQUIRED,
    PROJECTION_CONSENT_REQUIRED,
    CAPTURE_FAILED,
}

internal data class PresetAudioCaptureFailure(
    val reason: PresetAudioCaptureFailureReason,
    val detail: String? = null,
)

