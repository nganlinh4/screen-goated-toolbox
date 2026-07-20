package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorGroundingFrameStore
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.PhoneControlVisualProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.atomic.AtomicBoolean

internal class PhoneControlScreenStreamer(
    private val running: AtomicBoolean,
    private val transportReady: AtomicBoolean,
    private val screenFrames: Channel<String>,
    private val refreshRequests: Channel<Unit>,
    private val reconciliationFrameQueued: AtomicBoolean,
    private val statusPublisher: PhoneControlRuntimeStatusPublisher,
    private val currentTurnPhase: () -> PhoneControlTurnPhase,
    private val pendingWorkCount: () -> Int,
) {
    suspend fun run() {
        var lastFailureCode: String? = null
        var visibleFailurePublished = false
        var explicitRefreshPending = drainRefreshRequests()
        while (currentCoroutineContext().isActive && running.get()) {
            if (transportReady.get() && canSendAmbientScreen(pendingWorkCount())) {
                if (!PhoneControlAccessibilityProvider.isReady) {
                    if (lastFailureCode != CAPABILITY_UNAVAILABLE) {
                        lastFailureCode = CAPABILITY_UNAVAILABLE
                        Log.w(TAG, "screen_capture_degraded code=$CAPABILITY_UNAVAILABLE")
                        statusPublisher.publish(
                            phase = PhoneControlRuntimePhase.DEGRADED,
                            code = PhoneControlRuntimeCode.ACCESSIBILITY_UNAVAILABLE,
                            message = "Reconnect the SGT Accessibility service to share the screen.",
                        )
                        visibleFailurePublished = true
                    }
                } else {
                    val groundingFrame = UiDetectorGroundingFrameStore.takeForGeneration(
                        PhoneControlAccessibilityProvider.observationGeneration,
                    )
                    if (groundingFrame != null) {
                        explicitRefreshPending = queueFrame(groundingFrame, explicitRefreshPending)
                        if (lastFailureCode != null) {
                            statusPublisher.publishTurnPhase(currentTurnPhase())
                        }
                        lastFailureCode = null
                        visibleFailurePublished = false
                    } else {
                        when (val result = PhoneControlVisualProvider.captureStreamingFrame()) {
                            is VisualProviderResult.Success -> {
                                explicitRefreshPending = queueFrame(
                                    result.value.screenPayload,
                                    explicitRefreshPending,
                                )
                                if (lastFailureCode != null) {
                                    statusPublisher.publishTurnPhase(currentTurnPhase())
                                }
                                lastFailureCode = null
                                visibleFailurePublished = false
                            }
                            is VisualProviderResult.Failure -> {
                                val transient = isTransientScreenFrameFailure(result.code)
                                if (lastFailureCode != result.code) {
                                    lastFailureCode = result.code
                                    val state = if (transient) "waiting" else "degraded"
                                    val message = "screen_capture_$state code=${result.code}"
                                    if (transient) {
                                        Log.d(TAG, message)
                                    } else {
                                        Log.w(TAG, "$message retryable=${result.retryable}")
                                    }
                                }
                                if (transient) {
                                    if (visibleFailurePublished) {
                                        statusPublisher.publishTurnPhase(currentTurnPhase())
                                        visibleFailurePublished = false
                                    }
                                } else {
                                    statusPublisher.publishScreenFailure(result.message)
                                    visibleFailurePublished = true
                                }
                            }
                        }
                    }
                }
            }
            val requested = awaitRefreshRequest()
            explicitRefreshPending = explicitRefreshPending || requested
        }
    }

    private fun queueFrame(payload: String, explicitRefreshPending: Boolean): Boolean {
        if (!screenFrames.trySend(payload).isSuccess) return explicitRefreshPending
        if (explicitRefreshPending) reconciliationFrameQueued.set(true)
        return false
    }

    private fun drainRefreshRequests(): Boolean {
        var found = false
        while (refreshRequests.tryReceive().isSuccess) found = true
        return found
    }

    private suspend fun awaitRefreshRequest(): Boolean =
        withTimeoutOrNull(SCREEN_CAPTURE_INTERVAL_MS) {
            refreshRequests.receive()
            true
        } == true

    private companion object {
        const val TAG = "SGTPhoneControl"
        const val CAPABILITY_UNAVAILABLE = "capability_unavailable"
        const val SCREEN_CAPTURE_INTERVAL_MS = 1_500L
    }
}
