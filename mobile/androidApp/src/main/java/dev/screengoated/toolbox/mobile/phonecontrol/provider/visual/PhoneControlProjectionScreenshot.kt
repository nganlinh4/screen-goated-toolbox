package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionFrameResult
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityScreenshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds

internal suspend fun capturePhoneControlProjectionScreenshot(
    observationGeneration: Long,
): AccessibilityProviderResult<AccessibilityScreenshot> =
    PhoneControlOverlayExclusion.forCapture {
        when (val captured = PhoneControlProjectionProvider.capture()) {
            is PhoneControlProjectionFrameResult.Success -> {
                val bitmap = captured.bitmap
                AccessibilityProviderResult.Success(
                    AccessibilityScreenshot(
                        generation = observationGeneration,
                        visualRevision = PhoneControlAccessibilityProvider.currentVisualRevision,
                        capturedAtMs = captured.capturedAtMs,
                        bitmap = bitmap,
                        captureBounds = TargetBounds(0, 0, bitmap.width, bitmap.height),
                        windowId = null,
                    ),
                )
            }
            is PhoneControlProjectionFrameResult.Failure -> AccessibilityProviderResult.Failure(
                code = captured.code,
                message = "The required screen-sharing session could not provide a frame.",
                retryable = captured.retryable,
                requiredUserStep = if (captured.retryable) null else "restart_phone_control",
            )
        }
    }
