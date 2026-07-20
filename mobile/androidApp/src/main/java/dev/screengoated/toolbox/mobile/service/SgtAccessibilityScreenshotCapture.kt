package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.graphics.Bitmap
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.Display
import androidx.annotation.RequiresApi
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

internal class SgtAccessibilityScreenshotCapture(
    private val service: SgtAccessibilityService,
) {
    private val executor: ExecutorService by lazy { Executors.newSingleThreadExecutor() }
    private val mainHandler = Handler(Looper.getMainLooper())

    fun captureDisplay(callback: (ScreenshotCaptureResult) -> Unit) {
        if (!requireSupport(callback) || Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        capture(callback) { receiver ->
            service.takeScreenshot(Display.DEFAULT_DISPLAY, executor, receiver)
        }
    }

    fun captureWindow(
        accessibilityWindowId: Int,
        callback: (ScreenshotCaptureResult) -> Unit,
    ) {
        if (!requireSupport(callback)) return
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            post(callback, failure(ScreenshotCaptureFailureReason.API_TOO_OLD))
            return
        }
        captureWindowApi34(accessibilityWindowId, callback)
    }

    fun support(): ScreenshotCaptureSupport {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return ScreenshotCaptureSupport(false, ScreenshotCaptureFailureReason.API_TOO_OLD)
        }
        val capability = service.serviceInfo?.capabilities ?: 0
        return if (
            capability and AccessibilityServiceInfo.CAPABILITY_CAN_TAKE_SCREENSHOT != 0
        ) {
            ScreenshotCaptureSupport(true, null)
        } else {
            ScreenshotCaptureSupport(false, ScreenshotCaptureFailureReason.CAPABILITY_MISSING)
        }
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun captureWindowApi34(
        accessibilityWindowId: Int,
        callback: (ScreenshotCaptureResult) -> Unit,
    ) = capture(callback) { receiver ->
        service.takeScreenshotOfWindow(accessibilityWindowId, executor, receiver)
    }

    @RequiresApi(Build.VERSION_CODES.R)
    private fun capture(
        callback: (ScreenshotCaptureResult) -> Unit,
        dispatch: (AccessibilityService.TakeScreenshotCallback) -> Unit,
    ) {
        try {
            dispatch(
                object : AccessibilityService.TakeScreenshotCallback {
                    override fun onSuccess(result: AccessibilityService.ScreenshotResult) {
                        var captured: Bitmap? = null
                        try {
                            val hardware = Bitmap.wrapHardwareBuffer(
                                result.hardwareBuffer,
                                result.colorSpace,
                            )
                            if (hardware != null) {
                                captured = hardware.copy(Bitmap.Config.ARGB_8888, false)
                                hardware.recycle()
                            }
                        } catch (error: Exception) {
                            Log.e(TAG, "screenshot bitmap conversion failed", error)
                        } finally {
                            result.hardwareBuffer.close()
                        }
                        post(
                            callback,
                            captured?.let(ScreenshotCaptureResult::Success)
                                ?: failure(ScreenshotCaptureFailureReason.REQUEST_FAILED),
                        )
                    }

                    override fun onFailure(errorCode: Int) {
                        Log.d(TAG, "screenshot failed code=$errorCode")
                        post(callback, failure(mapFailure(errorCode)))
                    }
                },
            )
        } catch (error: SecurityException) {
            Log.e(TAG, "screenshot security failure", error)
            post(callback, failure(ScreenshotCaptureFailureReason.SECURITY_EXCEPTION))
        }
    }

    private fun requireSupport(callback: (ScreenshotCaptureResult) -> Unit): Boolean {
        val support = support()
        if (support.available) return true
        post(
            callback,
            failure(support.failureReason ?: ScreenshotCaptureFailureReason.REQUEST_FAILED),
        )
        return false
    }

    private fun post(
        callback: (ScreenshotCaptureResult) -> Unit,
        result: ScreenshotCaptureResult,
    ) = mainHandler.post { callback(result) }

    private fun failure(reason: ScreenshotCaptureFailureReason) =
        ScreenshotCaptureResult.Failure(reason)

    private fun mapFailure(errorCode: Int): ScreenshotCaptureFailureReason = when (errorCode) {
        AccessibilityService.ERROR_TAKE_SCREENSHOT_INTERVAL_TIME_SHORT ->
            ScreenshotCaptureFailureReason.RATE_LIMITED
        AccessibilityService.ERROR_TAKE_SCREENSHOT_INVALID_DISPLAY,
        AccessibilityService.ERROR_TAKE_SCREENSHOT_INVALID_WINDOW,
        -> ScreenshotCaptureFailureReason.INVALID_TARGET
        AccessibilityService.ERROR_TAKE_SCREENSHOT_NO_ACCESSIBILITY_ACCESS ->
            ScreenshotCaptureFailureReason.NO_ACCESSIBILITY_ACCESS
        AccessibilityService.ERROR_TAKE_SCREENSHOT_SECURE_WINDOW ->
            ScreenshotCaptureFailureReason.SECURE_WINDOW
        AccessibilityService.ERROR_TAKE_SCREENSHOT_INTERNAL_ERROR ->
            ScreenshotCaptureFailureReason.REQUEST_FAILED
        else -> ScreenshotCaptureFailureReason.REQUEST_FAILED
    }

    private companion object {
        const val TAG = "SgtAccessibility"
    }
}
