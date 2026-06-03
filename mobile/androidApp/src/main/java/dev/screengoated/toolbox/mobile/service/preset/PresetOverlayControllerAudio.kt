package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Rect
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import android.widget.Toast
import androidx.core.content.FileProvider
import dev.screengoated.toolbox.mobile.MainActivity
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
import dev.screengoated.toolbox.mobile.service.LiveTranslateService
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
    if (resolved.preset.audioProcessingMode == "realtime") {
        onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
        launchRealtimeAudioPreset(resolved)
        return
    }
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
) {
    when (failure.reason) {
        PresetAudioCaptureFailureReason.RECORD_PERMISSION_REQUIRED,
        -> {
            appContainer.audioPresetLaunchStore.set(
                AudioPresetLaunchRequest(
                    presetId = resolved.preset.id,
                    kind = AudioPresetLaunchKind.CAPTURE,
                ),
            )
            context.startActivity(
                Intent(context, MainActivity::class.java).apply {
                    addFlags(
                        Intent.FLAG_ACTIVITY_NEW_TASK or
                            Intent.FLAG_ACTIVITY_SINGLE_TOP or
                            Intent.FLAG_ACTIVITY_CLEAR_TOP,
                    )
                    putExtra(MainActivity.EXTRA_RESUME_PENDING_AUDIO_PRESET, true)
                },
            )
        }
        PresetAudioCaptureFailureReason.PROJECTION_CONSENT_REQUIRED -> {
            appContainer.audioPresetLaunchStore.set(
                AudioPresetLaunchRequest(
                    presetId = resolved.preset.id,
                    kind = AudioPresetLaunchKind.CAPTURE,
                ),
            )
            context.startActivity(
                dev.screengoated.toolbox.mobile.ProjectionConsentProxyActivity.resumeCapturePresetIntent(context),
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

internal fun PresetOverlayController.launchRealtimeAudioPreset(resolved: ResolvedPreset) {
    val phase = appContainer.repository.state.value.phase
    val activeRealtimePresetId = appContainer.audioPresetLaunchStore.activeRealtimePresetId()
    if (
        activeRealtimePresetId == resolved.preset.id &&
        appContainer.repository.isTransientSessionConfigActive() &&
        phase in setOf(
            dev.screengoated.toolbox.mobile.shared.live.SessionPhase.STARTING,
            dev.screengoated.toolbox.mobile.shared.live.SessionPhase.LISTENING,
            dev.screengoated.toolbox.mobile.shared.live.SessionPhase.TRANSLATING,
        )
    ) {
        LiveTranslateService.stop(context)
        appContainer.audioPresetLaunchStore.setActiveRealtimePresetId(null)
        return
    }
    appContainer.audioPresetLaunchStore.set(
        AudioPresetLaunchRequest(
            presetId = resolved.preset.id,
            kind = AudioPresetLaunchKind.REALTIME,
        ),
    )
    context.startActivity(
        Intent(context, MainActivity::class.java).apply {
            addFlags(
                Intent.FLAG_ACTIVITY_NEW_TASK or
                    Intent.FLAG_ACTIVITY_SINGLE_TOP or
                    Intent.FLAG_ACTIVITY_CLEAR_TOP,
            )
            putExtra(MainActivity.EXTRA_RESUME_PENDING_AUDIO_PRESET, true)
        },
    )
}

