package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import android.view.Display
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.maskPhoneControlOverlay
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionFrameResult
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityScreenshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.captureAmbientExternalWindowScreenshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlScreenPayload
import dev.screengoated.toolbox.mobile.phonecontrol.session.encodePhoneControlScreenImage
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import java.util.concurrent.atomic.AtomicBoolean

internal suspend fun capturePhoneControlProjectionScreenshot(
    observationGeneration: Long,
): AccessibilityProviderResult<AccessibilityScreenshot> =
    when (val captured = PhoneControlProjectionProvider.capture()) {
        is PhoneControlProjectionFrameResult.Success -> {
            logProjectionRoute()
            val bitmap = maskPhoneControlOverlay(captured.bitmap)
            AccessibilityProviderResult.Success(
                AccessibilityScreenshot(
                    generation = observationGeneration,
                    visualRevision = PhoneControlAccessibilityProvider.currentVisualRevision,
                    capturedAtMs = captured.capturedAtMs,
                    bitmap = bitmap,
                    captureBounds = TargetBounds(0, 0, bitmap.width, bitmap.height),
                    windowId = null,
                    captureProvider = "media_projection",
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

internal suspend fun captureProjectionOnlyStreamingFrame(): VisualProviderResult<VisualFrame> =
    when (val captured = PhoneControlProjectionProvider.capture()) {
        is PhoneControlProjectionFrameResult.Failure -> VisualProviderResult.Failure(
            code = captured.code,
            message = "The required screen-sharing session could not provide a frame.",
            retryable = captured.retryable,
            requiredUserStep = if (captured.retryable) null else "restart_phone_control",
        )
        is PhoneControlProjectionFrameResult.Success -> {
            logProjectionRoute()
            val bitmap = maskPhoneControlOverlay(captured.bitmap)
            try {
                val generation =
                    PhoneControlAccessibilityProvider.observationGeneration.coerceAtLeast(1L)
                val revision =
                    PhoneControlAccessibilityProvider.currentVisualRevision.coerceAtLeast(1L)
                val bounds = TargetBounds(0, 0, bitmap.width, bitmap.height)
                val imageBytes = encodePhoneControlScreenImage(bitmap)
                VisualProviderResult.Success(
                    VisualFrame(
                        identity = VisualFrameIdentity(
                            observationGeneration = generation,
                            visualRevision = revision,
                            displayId = Display.DEFAULT_DISPLAY,
                            windowId = null,
                            packageOrSurface = "android-display-${Display.DEFAULT_DISPLAY}",
                            cropBounds = bounds,
                            captureWidth = bitmap.width,
                            captureHeight = bitmap.height,
                            rotation = captured.rotation,
                            densityDpi = captured.densityDpi,
                            capturedAtMs = captured.capturedAtMs,
                            viewKind = VisualViewKind.WHOLE_DISPLAY,
                            clean = true,
                            grid = null,
                            captureProvider = "media_projection",
                        ),
                        screenPayload = buildPhoneControlScreenPayload(imageBytes),
                        imageBytes = imageBytes,
                    ),
                )
            } finally {
                bitmap.recycle()
            }
        }
    }

internal suspend fun captureLeaseFreeWindowStreamingFrame(): VisualProviderResult<VisualFrame> =
    when (val captured = captureAmbientExternalWindowScreenshot()) {
        is AccessibilityProviderResult.Failure -> VisualProviderResult.Failure(
            code = captured.code,
            message = captured.message,
            retryable = captured.retryable,
            requiredUserStep = captured.requiredUserStep,
            freshObservationRequired = captured.freshObservationRequired,
        )
        is AccessibilityProviderResult.Success -> {
            val frame = captured.value
            val screenshot = frame.screenshot
            try {
                val imageBytes = encodePhoneControlScreenImage(screenshot.bitmap)
                VisualProviderResult.Success(
                    VisualFrame(
                        identity = VisualFrameIdentity(
                            observationGeneration = screenshot.generation,
                            visualRevision = screenshot.visualRevision,
                            displayId = frame.displayId,
                            windowId = screenshot.windowId,
                            packageOrSurface = frame.packageOrSurface,
                            cropBounds = screenshot.captureBounds,
                            captureWidth = screenshot.bitmap.width,
                            captureHeight = screenshot.bitmap.height,
                            rotation = frame.rotation,
                            densityDpi = frame.densityDpi,
                            capturedAtMs = screenshot.capturedAtMs,
                            viewKind = VisualViewKind.ACTIVE_SURFACE,
                            clean = true,
                            grid = null,
                            captureProvider = screenshot.captureProvider,
                        ),
                        screenPayload = buildPhoneControlScreenPayload(imageBytes),
                        imageBytes = imageBytes,
                    ),
                )
            } finally {
                screenshot.bitmap.recycle()
            }
        }
    }

private fun logProjectionRoute() {
    if (projectionRouteLogged.compareAndSet(false, true)) {
        Log.i(TAG, "screenshot_route route=media_projection overlay_mutated=false")
    }
}

private val projectionRouteLogged = AtomicBoolean(false)
private const val TAG = "SGTPhoneControlVisual"
