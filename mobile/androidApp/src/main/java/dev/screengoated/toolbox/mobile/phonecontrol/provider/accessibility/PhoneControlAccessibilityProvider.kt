package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.graphics.Bitmap
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import android.view.accessibility.AccessibilityEvent
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.service.ScreenshotCaptureResult
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import kotlinx.coroutines.suspendCancellableCoroutine
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.resume

internal sealed interface AccessibilityProviderResult<out T> {
    data class Success<T>(val value: T) : AccessibilityProviderResult<T>

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean,
        val freshObservationRequired: Boolean = false,
        val effect: EffectCertainty = EffectCertainty.PROVEN_NO_EFFECT,
        val requiredUserStep: String? = null,
    ) : AccessibilityProviderResult<Nothing>
}

internal data class AccessibilityScreenshot(
    val generation: Long,
    val visualRevision: Long,
    val capturedAtMs: Long,
    val bitmap: Bitmap,
    val captureBounds: TargetBounds,
    val windowId: Long?,
)

internal object PhoneControlAccessibilityProvider {
    private val generation = AtomicLong(1L)
    private val visualRevision = AtomicLong(1L)
    private val lock = Any()
    private val mainHandler = Handler(Looper.getMainLooper())
    private val loggedWindowCapture = AtomicBoolean(false)
    private val loggedDisplayCapture = AtomicBoolean(false)

    @Volatile
    private var service: SgtAccessibilityService? = null

    @Volatile
    private var lastInvalidation: String = "initial"

    private var latestCapture: AccessibilityCapture? = null

    val isReady: Boolean
        get() = service != null

    val observationGeneration: Long
        get() = generation.get()

    val currentVisualRevision: Long
        get() = visualRevision.get()

    internal val currentServicePackage: String?
        get() = service?.packageName

    fun attach(candidate: SgtAccessibilityService) {
        service = candidate
        invalidate("service_connected")
    }

    fun detach(candidate: SgtAccessibilityService) {
        if (service === candidate) {
            service = null
            invalidate("service_destroyed")
        }
    }

    fun onAccessibilityEvent(
        candidate: SgtAccessibilityService,
        event: AccessibilityEvent?,
    ) {
        if (service !== candidate || event == null) return
        val impact = accessibilityInvalidationImpact(event.eventType, event.contentChangeTypes)
        if (impact == AccessibilityInvalidationImpact.NONE) return
        val source = event.packageName?.toString().orEmpty().ifBlank { "unknown" }
        val windows = synchronized(lock) {
            latestCapture?.observation?.windows.orEmpty()
        }
        if (shouldIgnoreControllerOverlayEvent(
                eventWindowId = event.windowId,
                eventPackage = source,
                servicePackage = candidate.packageName,
                windows = windows,
                controllerTransitionActive = PhoneControlOverlayExclusion.controllerTransitionActive,
            )
        ) {
            return
        }
        AccessibilityInvalidationDiagnostics.record(
            impact = impact,
            eventType = event.eventType,
            contentChangeTypes = event.contentChangeTypes,
            windowId = event.windowId,
            sourcePackage = source,
            generation = observationGeneration,
            visualRevision = currentVisualRevision,
        )
        if (impact == AccessibilityInvalidationImpact.HARD) {
            invalidate("event:${event.eventType}:$source")
        } else {
            advanceVisualRevision()
        }
    }

    suspend fun observe(maxElements: Int = 400): AccessibilityProviderResult<AccessibilityObservation> {
        return observeInternal(maxElements, publishActionLeases = true)
    }

    suspend fun observeForVisual(
        maxElements: Int = 400,
    ): AccessibilityProviderResult<AccessibilityObservation> {
        return observeInternal(maxElements, publishActionLeases = false)
    }

    private suspend fun observeInternal(
        maxElements: Int,
        publishActionLeases: Boolean,
    ): AccessibilityProviderResult<AccessibilityObservation> {
        return onServiceMain { activeService ->
            var acceptedGeneration = generation.get()
            var capture = captureAccessibilitySurface(activeService, acceptedGeneration, maxElements)
            if (generation.get() != acceptedGeneration) {
                acceptedGeneration = generation.get()
                capture = captureAccessibilitySurface(activeService, acceptedGeneration, maxElements)
            }
            if (generation.get() != acceptedGeneration) {
                return@onServiceMain AccessibilityProviderResult.Failure(
                    code = "surface_unstable",
                    message = "The visible surface changed while it was being observed.",
                    retryable = true,
                    freshObservationRequired = true,
                )
            }
            if (publishActionLeases) {
                synchronized(lock) {
                    latestCapture = capture
                }
            }
            AccessibilityProviderResult.Success(capture.observation)
        }
    }

    suspend fun act(
        targetId: Int,
        verb: AccessibilityActionVerb,
        value: String? = null,
        confirmed: Boolean = false,
    ): AccessibilityProviderResult<AccessibilityActionOutcome> =
        performAccessibilityAction(this, targetId, verb, value, confirmed)

    suspend fun runReversibleLocalControlSelfTest(
        observationGeneration: Long,
    ): AccessibilityProviderResult<AccessibilityReversibleSelfTestOutcome> =
        performReversibleLocalControlSelfTest(this, observationGeneration)

    suspend fun focusedTextTarget(
        surface: AndroidSurfaceIdentity? = null,
    ): AccessibilityProviderResult<AccessibilityTextTarget> =
        findFocusedAccessibilityTextTarget(this, surface)

    suspend fun typeText(
        target: AccessibilityTextTarget,
        text: String,
        slow: Boolean,
        pressEnter: Boolean,
    ): AccessibilityProviderResult<AccessibilityTextOutcome> =
        performAccessibilityTextEdit(this, target, text, slow, pressEnter)

    suspend fun sendKeys(
        target: AccessibilityTextTarget,
        groups: List<AccessibilityKeyGroup>,
        holdMs: Long,
    ): AccessibilityProviderResult<AccessibilityTextOutcome> =
        performAccessibilityKeySequence(this, target, groups, holdMs)

    suspend fun prepareCommandDispatch(): AccessibilityProviderResult<AccessibilityCommandDispatchLease> =
        when (val observed = observe(maxElements = 1)) {
            is AccessibilityProviderResult.Failure -> observed
            is AccessibilityProviderResult.Success -> {
                observed.value.commandDispatchAuthorityFailure()
                    ?: AccessibilityProviderResult.Success(
                        AccessibilityCommandDispatchLease(observed.value.generation),
                    )
            }
        }

    fun validateCommandDispatch(
        lease: AccessibilityCommandDispatchLease,
    ): AccessibilityProviderResult.Failure? {
        val observation = synchronized(lock) { latestCapture?.observation }
        if (observation == null ||
            observation.generation != lease.observationGeneration ||
            observationGeneration != lease.observationGeneration
        ) {
            return staleMutation("The visible surface changed before command dispatch.")
        }
        return observation.commandDispatchAuthorityFailure()
    }

    suspend fun click(
        lease: AccessibilitySurfaceLease,
        x: Float,
        y: Float,
        confirmed: Boolean = false,
        expectedVisualRevision: Long? = null,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        PhoneControlOverlayExclusion.forPoint(x, y) {
            dispatchAccessibilityClick(
                this,
                lease,
                x,
                y,
                confirmed,
                expectedVisualRevision,
            )
        }

    suspend fun swipe(
        lease: AccessibilitySurfaceLease,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        durationMs: Long,
        kind: AccessibilityMutationKind,
        confirmed: Boolean = false,
        expectedVisualRevision: Long? = null,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        PhoneControlOverlayExclusion.forSegment(fromX, fromY, toX, toY) {
            dispatchAccessibilitySwipe(
                this,
                lease,
                fromX,
                fromY,
                toX,
                toY,
                durationMs,
                kind,
                confirmed,
                expectedVisualRevision,
            )
        }

    suspend fun globalAction(
        lease: AccessibilitySurfaceLease,
        action: Int,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        performAccessibilityGlobalAction(this, lease, action)

    suspend fun screenshot(
        windowId: Long? = null,
        windowBounds: TargetBounds? = null,
    ): AccessibilityProviderResult<AccessibilityScreenshot> {
        val activeService = service ?: return unavailable()
        val platformWindowId = windowId?.takeIf { it in Int.MIN_VALUE..Int.MAX_VALUE }?.toInt()
        return if (
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE &&
            platformWindowId != null &&
            windowBounds != null
        ) {
            if (loggedWindowCapture.compareAndSet(false, true)) {
                Log.i(TAG, "screenshot_route route=window_scoped overlay_mutated=false")
            }
            captureScreenshot(windowBounds, windowId) { callback ->
                activeService.captureWindowScreenshot(platformWindowId, callback)
            }
        } else {
            if (loggedDisplayCapture.compareAndSet(false, true)) {
                Log.i(TAG, "screenshot_route route=display_scoped overlay_suppressed=true")
            }
            PhoneControlOverlayExclusion.forCapture {
                captureScreenshot(null, null, activeService::captureScreenshot)
            }
        }
    }

    private suspend fun captureScreenshot(
        captureBounds: TargetBounds?,
        windowId: Long?,
        dispatch: ((ScreenshotCaptureResult) -> Unit) -> Unit,
    ): AccessibilityProviderResult<AccessibilityScreenshot> {
        return suspendCancellableCoroutine { continuation ->
            dispatch { result ->
                if (!continuation.isActive) return@dispatch
                val mapped = when (result) {
                    is ScreenshotCaptureResult.Success -> AccessibilityProviderResult.Success(
                        AccessibilityScreenshot(
                            generation = generation.get(),
                            visualRevision = visualRevision.get(),
                            capturedAtMs = SystemClock.elapsedRealtime(),
                            bitmap = result.bitmap,
                            captureBounds = captureBounds ?: TargetBounds(
                                0,
                                0,
                                result.bitmap.width,
                                result.bitmap.height,
                            ),
                            windowId = windowId,
                        ),
                    )
                    is ScreenshotCaptureResult.Failure -> AccessibilityProviderResult.Failure(
                        code = "screenshot_${result.reason.name.lowercase()}",
                        message = "Accessibility screenshot failed: ${result.reason.name.lowercase()}.",
                        retryable = result.reason.name in RETRYABLE_SCREENSHOT_FAILURES,
                    )
                }
                continuation.resume(mapped)
            }
        }
    }

    internal fun currentLease(targetId: Int): AccessibilityTargetLease? = synchronized(lock) {
        latestCapture?.leases?.get(targetId)
    }

    internal fun currentElement(targetId: Int): AccessibilityElement? = synchronized(lock) {
        latestCapture?.observation?.elements?.singleOrNull { element -> element.id == targetId }
    }

    internal fun currentObservation(): AccessibilityObservation? = synchronized(lock) {
        latestCapture?.observation
    }

    internal fun currentCaptureGeneration(): Long? = synchronized(lock) {
        latestCapture?.observation?.generation
    }

    internal fun currentCaptureForLocalSelfTest(
        expectedGeneration: Long,
    ): AccessibilityCapture? = synchronized(lock) {
        latestCapture?.takeIf { capture ->
            capture.observation.generation == expectedGeneration &&
                generation.get() == expectedGeneration
        }
    }

    internal fun validateTargetMutation(
        lease: AccessibilityTargetLease,
        kind: AccessibilityMutationKind,
        confirmed: Boolean,
    ): AccessibilityProviderResult.Failure? {
        val current = synchronized(lock) { latestCapture?.leases?.get(lease.id) }
        if (current != lease || observationGeneration != lease.identity.snapshotGeneration) {
            return staleMutation("The target does not belong to the current observation.")
        }
        if (lease.identity.displayId != lease.surfaceLease.displayId ||
            lease.identity.windowId != lease.surfaceLease.windowId ||
            lease.identity.snapshotGeneration != lease.surfaceLease.observationGeneration
        ) {
            return staleMutation("The target and surface identities do not match.")
        }
        validateSurfaceMutation(
            lease.surfaceLease,
            kind,
            confirmed,
            lease.identity.bounds,
        )?.let { return it }
        return authorityFailure(lease.authority, kind, confirmed)
    }

    internal fun validateSurfaceMutation(
        lease: AccessibilitySurfaceLease,
        kind: AccessibilityMutationKind,
        confirmed: Boolean,
        affectedBounds: TargetBounds?,
    ): AccessibilityProviderResult.Failure? {
        val observation = synchronized(lock) {
            latestCapture?.observation?.takeIf { current ->
                current.generation == lease.observationGeneration
            }
        }
        return validateSurfaceMutationLease(
            observation,
            observationGeneration,
            lease,
            kind,
            confirmed,
            affectedBounds,
        )
    }

    internal fun validateVisualRevision(expected: Long?): AccessibilityProviderResult.Failure? {
        return visualRevisionFailure(expected, visualRevision.get())
    }

    internal fun invalidate(reason: String): Long {
        lastInvalidation = reason
        advanceVisualRevision()
        synchronized(lock) {
            latestCapture = null
        }
        return generation.updateAndGet { current ->
            if (current == Long.MAX_VALUE) 1L else current + 1L
        }
    }

    private fun advanceVisualRevision(): Long = visualRevision.updateAndGet { current ->
        if (current == Long.MAX_VALUE) 1L else current + 1L
    }

    internal suspend fun <T> onServiceMain(
        failureEffect: EffectCertainty = EffectCertainty.PROVEN_NO_EFFECT,
        block: (SgtAccessibilityService) -> AccessibilityProviderResult<T>,
    ): AccessibilityProviderResult<T> {
        val activeService = service ?: return unavailable()
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return if (service === activeService) {
                guardedProviderCall(activeService, failureEffect, block)
            } else {
                unavailable()
            }
        }
        return suspendCancellableCoroutine { continuation ->
            mainHandler.post {
                if (!continuation.isActive) return@post
                val result = if (service === activeService) {
                    guardedProviderCall(activeService, failureEffect, block)
                } else {
                    unavailable()
                }
                continuation.resume(result)
            }
        }
    }

    private fun <T> guardedProviderCall(
        activeService: SgtAccessibilityService,
        failureEffect: EffectCertainty,
        block: (SgtAccessibilityService) -> AccessibilityProviderResult<T>,
    ): AccessibilityProviderResult<T> = try {
        block(activeService)
    } catch (error: Exception) {
        val origin = error.stackTrace.firstOrNull()?.let { frame ->
            "${frame.className}.${frame.methodName}:${frame.lineNumber}"
        } ?: "unknown"
        Log.e(
            TAG,
            "provider_operation_failed type=${error.javaClass.simpleName} origin=$origin",
        )
        if (failureEffect != EffectCertainty.PROVEN_NO_EFFECT) {
            invalidate("provider_operation_failed")
        }
        AccessibilityProviderResult.Failure(
            code = "provider_failed",
            message = "The Accessibility provider could not complete the platform operation.",
            retryable = true,
            freshObservationRequired = true,
            effect = failureEffect,
        )
    }

    private fun unavailable(): AccessibilityProviderResult.Failure = AccessibilityProviderResult.Failure(
        code = "capability_unavailable",
        message = "The SGT Accessibility service is not connected.",
        retryable = true,
        freshObservationRequired = true,
    )

    private fun staleMutation(message: String) = AccessibilityProviderResult.Failure(
        code = "stale_target",
        message = message,
        retryable = true,
        freshObservationRequired = true,
    )

    private val RETRYABLE_SCREENSHOT_FAILURES = setOf(
        "RATE_LIMITED",
        "REQUEST_FAILED",
        "SERVICE_UNAVAILABLE",
    )

    private const val TAG = "SGTPhoneControlAccessibility"
}

internal fun visualRevisionFailure(
    expected: Long?,
    current: Long,
): AccessibilityProviderResult.Failure? {
    if (expected == null || expected == current) return null
    return AccessibilityProviderResult.Failure(
        code = "stale_frame",
        message = "The visual content changed before coordinate dispatch.",
        retryable = true,
        freshObservationRequired = true,
    )
}

internal fun isKnownControllerOverlayEvent(
    eventWindowId: Int,
    eventPackage: String?,
    servicePackage: String,
    windows: List<AccessibilityWindowSnapshot>,
): Boolean {
    if (eventWindowId < 0 || eventPackage != servicePackage) return false
    return windows.any { window ->
        window.id == eventWindowId &&
            window.packageName == servicePackage &&
            window.controllerOwned &&
            !isApplicationContentWindowType(window.type)
    }
}

internal fun shouldIgnoreControllerOverlayEvent(
    eventWindowId: Int,
    eventPackage: String?,
    servicePackage: String,
    windows: List<AccessibilityWindowSnapshot>,
    controllerTransitionActive: Boolean,
): Boolean {
    if (isKnownControllerOverlayEvent(eventWindowId, eventPackage, servicePackage, windows)) {
        return true
    }
    return controllerTransitionActive &&
        eventWindowId < 0 &&
        eventPackage == servicePackage
}
