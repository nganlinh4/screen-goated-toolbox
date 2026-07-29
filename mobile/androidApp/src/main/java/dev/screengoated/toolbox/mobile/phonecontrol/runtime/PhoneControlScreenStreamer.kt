package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding.VisualGroundingFrameStore
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
    private val visualEvidenceEnabled: AtomicBoolean,
    private val screenFrames: Channel<String>,
    private val refreshRequests: Channel<Unit>,
    private val reconciliationFrameQueued: AtomicBoolean,
    private val statusPublisher: PhoneControlRuntimeStatusPublisher,
    private val currentTurnPhase: () -> PhoneControlTurnPhase,
    private val pendingWorkCount: () -> Int,
) {
    suspend fun run() {
        var lastFailureCode: String? = null
        var lastCaptureRoute: String? = null
        var visibleFailurePublished = false
        val failurePolicy = ScreenCaptureFailurePolicy()
        var explicitRefreshPending = drainRefreshRequests()
        while (currentCoroutineContext().isActive && running.get()) {
            if (visualEvidenceEnabled.get() &&
                transportReady.get() &&
                canSendAmbientScreen(pendingWorkCount())
            ) {
                val groundingFrame = VisualGroundingFrameStore.take(
                    PhoneControlAccessibilityProvider.observationGeneration,
                )
                if (groundingFrame != null) {
                    lastCaptureRoute = logCaptureRouteTransition(
                        lastCaptureRoute,
                        "current_frame_vision",
                        "grounding_frame",
                    )
                    explicitRefreshPending = queueFrame(groundingFrame, explicitRefreshPending)
                    if (lastFailureCode != null) {
                        statusPublisher.publishTurnPhase(currentTurnPhase())
                    }
                    failurePolicy.reset()
                    lastFailureCode = null
                    visibleFailurePublished = false
                } else {
                    when (val result = PhoneControlVisualProvider.captureStreamingFrame()) {
                        is VisualProviderResult.Success -> {
                            val identity = result.value.identity
                            lastCaptureRoute = logCaptureRouteTransition(
                                lastCaptureRoute,
                                identity.captureProvider,
                                screenCaptureRoute(
                                    captureProvider = identity.captureProvider,
                                    hasCoordinateLease = identity.grid != null,
                                ),
                            )
                            explicitRefreshPending = queueFrame(
                                result.value.screenPayload,
                                explicitRefreshPending,
                            )
                            if (lastFailureCode != null) {
                                statusPublisher.publishTurnPhase(currentTurnPhase())
                            }
                            failurePolicy.reset()
                            lastFailureCode = null
                            visibleFailurePublished = false
                        }
                        is VisualProviderResult.Failure -> {
                            val shouldPublish = visibleFailurePublished ||
                                failurePolicy.shouldPublish(result.code, result.retryable)
                            if (lastFailureCode != result.code ||
                                shouldPublish != visibleFailurePublished
                            ) {
                                lastFailureCode = result.code
                                val state = if (shouldPublish) "degraded" else "waiting"
                                val message = "screen_capture_$state code=${result.code}"
                                if (shouldPublish) {
                                    Log.w(TAG, "$message retryable=${result.retryable}")
                                } else {
                                    Log.d(TAG, message)
                                }
                            }
                            if (shouldPublish && !visibleFailurePublished) {
                                statusPublisher.publishScreenFailure(result.message)
                            }
                            visibleFailurePublished = shouldPublish
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

    private fun logCaptureRouteTransition(
        previous: String?,
        provider: String,
        route: String,
    ): String {
        val current = "$provider/$route"
        if (previous != current) {
            Log.i(
                TAG,
                "screen_capture_route provider=$provider route=$route overlay_mutated=false",
            )
        }
        return current
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
        const val SCREEN_CAPTURE_INTERVAL_MS = 1_500L
    }
}

internal fun screenCaptureRoute(
    captureProvider: String,
    hasCoordinateLease: Boolean,
): String = when {
    captureProvider == "media_projection" && !hasCoordinateLease -> "projection_only"
    captureProvider == "media_projection" -> "whole_display"
    captureProvider == "accessibility_window_lease_free" -> "window_lease_free"
    else -> "semantic_visual"
}
