package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Rect
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import android.widget.Toast
import androidx.core.content.FileProvider
import dev.screengoated.toolbox.mobile.AudioPresetPermissionActivity
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchKind
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchRequest
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.resolvePrompt
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.service.ScreenshotCaptureFailureReason
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import java.io.File
import kotlin.math.roundToInt

// Audio-launch flow extracted from PresetOverlayController.
internal fun PresetOverlayController.launchAudioPreset(resolved: ResolvedPreset) {
    val foregroundMode = if (resolved.preset.audioSource == "device") {
        PresetAudioForegroundMode.MEDIA_PROJECTION
    } else {
        PresetAudioForegroundMode.MICROPHONE
    }
    onAudioCaptureForegroundModeChanged(foregroundMode)
    audioCaptureSession.start(
        resolvedPreset = resolved,
        onRecordingComplete = { capture ->
            onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
            presetRepository.resetState()
            presetRepository.executePreset(
                resolved.preset,
                PresetInput.Audio(
                    wavBytes = capture.wavBytes,
                    precomputedTranscript = capture.precomputedTranscript,
                    isStreamingResult = capture.isStreamingResult,
                ),
            )
        },
        onCancelled = {
            onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
            if (!resultModule.hasResults()) {
                activePreset = null
            }
        },
        onFailure = { failure ->
            onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
            handleAudioCaptureFailure(resolved, failure)
        },
    )
}

internal fun PresetOverlayController.handleAudioCaptureFailure(
    resolved: ResolvedPreset,
    failure: PresetAudioCaptureFailure,
    kind: AudioPresetLaunchKind = AudioPresetLaunchKind.CAPTURE,
    inputWindowId: String? = null,
) {
    when (failure.reason) {
        PresetAudioCaptureFailureReason.RECORD_PERMISSION_REQUIRED,
        -> {
            val request = AudioPresetLaunchRequest(
                presetId = resolved.preset.id,
                kind = kind,
                ownerId = audioRecoveryOwnerId,
                inputWindowId = inputWindowId,
            )
            if (!appContainer.audioPresetLaunchStore.set(request)) return
            context.startActivity(
                AudioPresetPermissionActivity.resumeCapturePresetIntent(context, request.requestId),
            )
        }
        PresetAudioCaptureFailureReason.PROJECTION_CONSENT_REQUIRED -> {
            val request = AudioPresetLaunchRequest(
                presetId = resolved.preset.id,
                kind = kind,
                ownerId = audioRecoveryOwnerId,
                inputWindowId = inputWindowId,
            )
            if (!appContainer.audioPresetLaunchStore.set(request)) return
            context.startActivity(
                dev.screengoated.toolbox.mobile.ProjectionConsentProxyActivity.resumeCapturePresetIntent(context, request.requestId),
            )
        }
        PresetAudioCaptureFailureReason.CAPTURE_FAILED -> {
            Toast.makeText(
                context,
                localized(
                    "Audio capture failed.",
                    "Không thể ghi âm.",
                    "오디오 캡처에 실패했습니다.",
                ),
                Toast.LENGTH_SHORT,
                ).show()
            activePreset = null
        }
    }
}

internal fun PresetOverlayController.requiresAccessibilityForAudioAutoPaste(resolved: ResolvedPreset): Boolean {
    if (!resolved.preset.autoPaste) {
        return false
    }
    return resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC ||
        resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO
}
