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

// Image-capture flow extracted from PresetOverlayController.
internal fun PresetOverlayController.launchImagePreset(
    resolved: ResolvedPreset,
    continuousMode: Boolean,
) {
    val trace = newImageCaptureTrace(
        resolved = resolved,
        continuousMode = continuousMode,
        source = "preset_press",
    )
    logImageCaptureTrace(trace, "preset_pressed")
    if (continuousMode && imageContinuousPresetId == resolved.preset.id) {
        stopImageContinuousMode(showToast = true)
        return
    }
    SgtAccessibilityService.currentScreenshotSupport().failureReason?.let { reason ->
        handleImageCaptureFailure(reason, continuousMode = false)
        return
    }

    imageContinuousPresetId = if (continuousMode) resolved.preset.id else null
    imageContinuousRearmPending = false
    if (continuousMode) {
        Toast.makeText(
            context,
            localized(
                "Image continuous mode armed.",
                "Đã bật chế độ chụp ảnh liên tục.",
                "이미지 연속 모드가 활성화되었습니다.",
            ),
            Toast.LENGTH_SHORT,
        ).show()
    }
    startImageCaptureSession(resolved, continuousMode, trace)
}

internal fun PresetOverlayController.startImageCaptureSession(
    resolved: ResolvedPreset,
    continuousMode: Boolean,
    trace: ImageCaptureTrace,
) {
    processingIndicator.dismiss()
    imageCaptureSession.start(
        resolvedPreset = resolved,
        trace = trace,
        onSelectionConfirmed = { pngBytes ->
            if (resolved.preset.promptMode == "dynamic") {
                pendingImageBytes = pngBytes
                inputModule.open(resolved)
            } else {
                presetRepository.executePreset(resolved.preset, PresetInput.Image(pngBytes))
                imageContinuousRearmPending = continuousMode
            }
        },
        onColorPicked = { hexColor ->
            copyColorToClipboard(hexColor)
            if (continuousMode && imageContinuousPresetId == resolved.preset.id) {
                startImageCaptureSession(
                    resolved = resolved,
                    continuousMode = true,
                    trace = newImageCaptureTrace(
                        resolved = resolved,
                        continuousMode = true,
                        source = "color_pick_rearm",
                    ),
                )
            } else {
                activePreset = null
            }
        },
        onCancelled = {
            pendingImageBytes = null
            imageContinuousRearmPending = false
            if (continuousMode) {
                stopImageContinuousMode(showToast = false)
            } else {
                activePreset = null
            }
        },
        onCaptureFailure = { reason ->
            imageContinuousRearmPending = false
            handleImageCaptureFailure(reason, continuousMode)
        },
    )
}

internal fun PresetOverlayController.maybeRearmImageContinuous() {
    val presetId = imageContinuousPresetId ?: return
    if (!imageContinuousRearmPending || imageCaptureSession.isActive || inputModule.hasWindow()) {
        return
    }
    val resolved = presetRepository.getResolvedPreset(presetId) ?: return
    imageContinuousRearmPending = false
    startImageCaptureSession(
        resolved = resolved,
        continuousMode = true,
        trace = newImageCaptureTrace(
            resolved = resolved,
            continuousMode = true,
            source = "continuous_rearm",
        ),
    )
}

internal fun PresetOverlayController.stopImageContinuousMode(showToast: Boolean) {
    val wasActive = imageContinuousPresetId != null
    imageContinuousPresetId = null
    imageContinuousRearmPending = false
    pendingImageBytes = null
    imageCaptureSession.destroy()
    if (showToast && wasActive) {
        Toast.makeText(
            context,
            localized(
                "Image continuous mode exited.",
                "Đã thoát chế độ chụp ảnh liên tục.",
                "이미지 연속 모드를 종료했습니다.",
            ),
            Toast.LENGTH_SHORT,
        ).show()
    }
}

internal fun PresetOverlayController.handleImageCaptureFailure(
    reason: ScreenshotCaptureFailureReason,
    continuousMode: Boolean,
) {
    if (continuousMode) {
        stopImageContinuousMode(showToast = false)
    }
    val message = when (reason) {
        ScreenshotCaptureFailureReason.API_TOO_OLD ->
            localized(
                "Image presets require Android 11 or later.",
                "Preset ảnh cần Android 11 trở lên.",
                "이미지 프리셋은 Android 11 이상이 필요합니다.",
            )

        ScreenshotCaptureFailureReason.SERVICE_UNAVAILABLE,
        ScreenshotCaptureFailureReason.CAPABILITY_MISSING,
        ScreenshotCaptureFailureReason.NO_ACCESSIBILITY_ACCESS,
        ScreenshotCaptureFailureReason.SECURITY_EXCEPTION,
        -> localized(
            "Accessibility screenshot permission is required. Opening Settings...",
            "Cần quyền chụp màn hình của Dịch vụ trợ năng. Đang mở Cài đặt...",
            "접근성 스크린샷 권한이 필요합니다. 설정을 여는 중...",
        )

        ScreenshotCaptureFailureReason.RATE_LIMITED ->
            localized(
                "Screenshot requested too quickly. Try again in a moment.",
                "Yêu cầu chụp quá nhanh. Hãy thử lại sau một lát.",
                "스크린샷 요청이 너무 빠릅니다. 잠시 후 다시 시도하세요.",
            )

        ScreenshotCaptureFailureReason.INVALID_TARGET ->
            localized(
                "Could not capture this screen.",
                "Không thể chụp màn hình này.",
                "이 화면을 캡처할 수 없습니다.",
            )

        ScreenshotCaptureFailureReason.SECURE_WINDOW ->
            localized(
                "This screen blocks screenshots.",
                "Màn hình này chặn chụp màn hình.",
                "이 화면은 스크린샷을 차단합니다.",
            )

        ScreenshotCaptureFailureReason.REQUEST_FAILED ->
            localized(
                "Could not capture screenshot.",
                "Không thể chụp màn hình.",
                "스크린샷을 캡처할 수 없습니다.",
            )
    }
    Toast.makeText(
        context,
        message,
        if (reason.opensAccessibilitySettings()) Toast.LENGTH_LONG else Toast.LENGTH_SHORT,
    ).show()
    if (reason.opensAccessibilitySettings()) {
        openAccessibilitySettings()
    }
}

internal fun PresetOverlayController.newImageCaptureTrace(
    resolved: ResolvedPreset,
    continuousMode: Boolean,
    source: String,
): ImageCaptureTrace {
    return ImageCaptureTrace(
        id = nextImageCaptureTraceId++,
        presetId = resolved.preset.id,
        startedAtMs = SystemClock.elapsedRealtime(),
        continuousMode = continuousMode,
        source = source,
    )
}

internal fun ScreenshotCaptureFailureReason.opensAccessibilitySettings(): Boolean {
    return this == ScreenshotCaptureFailureReason.SERVICE_UNAVAILABLE ||
        this == ScreenshotCaptureFailureReason.CAPABILITY_MISSING ||
        this == ScreenshotCaptureFailureReason.NO_ACCESSIBILITY_ACCESS ||
        this == ScreenshotCaptureFailureReason.SECURITY_EXCEPTION
}
