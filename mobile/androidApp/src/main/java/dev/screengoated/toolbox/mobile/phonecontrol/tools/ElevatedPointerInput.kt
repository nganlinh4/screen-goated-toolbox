package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import kotlin.coroutines.coroutineContext
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.roundToInt
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive

internal data class PointerInputOutcome(
    val providerId: String,
    val providerState: CapabilityState,
    val code: String,
    val generation: Long,
    val effect: EffectCertainty,
    val snapshotInvalidated: Boolean,
    val retryable: Boolean = false,
    val requiredUserStep: String? = null,
    val freshObservationRequired: Boolean = snapshotInvalidated,
    val message: String? = null,
)

internal interface ElevatedPointerInput {
    suspend fun tap(
        job: PhoneControlToolJobContext,
        lease: AccessibilitySurfaceLease,
        x: Float,
        y: Float,
        expectedVisualRevision: Long?,
        holdMs: Long? = null,
    ): PointerInputOutcome?

    suspend fun swipe(
        job: PhoneControlToolJobContext,
        lease: AccessibilitySurfaceLease,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        durationMs: Long,
        kind: AccessibilityMutationKind,
        expectedVisualRevision: Long?,
    ): PointerInputOutcome?
}

internal object NoElevatedPointerInput : ElevatedPointerInput {
    override suspend fun tap(
        job: PhoneControlToolJobContext,
        lease: AccessibilitySurfaceLease,
        x: Float,
        y: Float,
        expectedVisualRevision: Long?,
        holdMs: Long?,
    ): PointerInputOutcome? = null

    override suspend fun swipe(
        job: PhoneControlToolJobContext,
        lease: AccessibilitySurfaceLease,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        durationMs: Long,
        kind: AccessibilityMutationKind,
        expectedVisualRevision: Long?,
    ): PointerInputOutcome? = null
}

internal class AndroidElevatedPointerInput(context: Context) : ElevatedPointerInput {
    private val context = context.applicationContext

    override suspend fun tap(
        job: PhoneControlToolJobContext,
        lease: AccessibilitySurfaceLease,
        x: Float,
        y: Float,
        expectedVisualRevision: Long?,
        holdMs: Long?,
    ): PointerInputOutcome? {
        val kind = if (holdMs == null) {
            AccessibilityMutationKind.POINTER_ACTIVATE
        } else {
            AccessibilityMutationKind.LONG_PRESS
        }
        val bounds = pointBounds(x, y)
        val args = if (holdMs == null) {
            listOf("tap", x.roundToInt().toString(), y.roundToInt().toString())
        } else {
            listOf(
                "swipe",
                x.roundToInt().toString(),
                y.roundToInt().toString(),
                x.roundToInt().toString(),
                y.roundToInt().toString(),
                holdMs.toString(),
            )
        }
        return PhoneControlOverlayExclusion.forPoint(x, y) {
            execute(job, lease, kind, bounds, expectedVisualRevision, args)
        }
    }

    override suspend fun swipe(
        job: PhoneControlToolJobContext,
        lease: AccessibilitySurfaceLease,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        durationMs: Long,
        kind: AccessibilityMutationKind,
        expectedVisualRevision: Long?,
    ): PointerInputOutcome? = PhoneControlOverlayExclusion.forSegment(
        fromX,
        fromY,
        toX,
        toY,
    ) {
        execute(
            job = job,
            lease = lease,
            kind = kind,
            affectedBounds = gestureBounds(fromX, fromY, toX, toY),
            expectedVisualRevision = expectedVisualRevision,
            args = listOf(
                "swipe",
                fromX.roundToInt().toString(),
                fromY.roundToInt().toString(),
                toX.roundToInt().toString(),
                toY.roundToInt().toString(),
                durationMs.toString(),
            ),
        )
    }

    private suspend fun execute(
        job: PhoneControlToolJobContext,
        lease: AccessibilitySurfaceLease,
        kind: AccessibilityMutationKind,
        affectedBounds: TargetBounds,
        expectedVisualRevision: Long?,
        args: List<String>,
    ): PointerInputOutcome? {
        val provider = selectedProvider() ?: return null
        val availability = provider.awaitReady(context)
        if (availability.state != CapabilityState.READY) return null
        PhoneControlAccessibilityProvider.validateVisualRevision(expectedVisualRevision)
            ?.let(::accessibilityGuardOutcome)
            ?.let { return it }
        PhoneControlAccessibilityProvider.validateSurfaceMutation(
            lease,
            kind,
            confirmed = false,
            affectedBounds = affectedBounds,
        )?.let(::accessibilityGuardOutcome)?.let { return it }
        val result = provider.executeAuthorized(
            context = context,
            effectOwner = job.effectOwner,
            program = INPUT_PROGRAM,
            args = args,
            cwd = COMMAND_CWD,
            timeoutMs = INPUT_TIMEOUT_MS,
        )
        coroutineContext.ensureActive()
        return commandOutcome(provider.providerId, result)
    }

    private fun selectedProvider(): PrivilegedCommandProvider? {
        val selected = PhoneControlPowerPreferences.current(context)?.elevatedProviderId
            ?: return null
        if (selected !in PhoneControlProviderRegistry.providersFor(context, POINTER_CAPABILITY)) {
            return null
        }
        return PrivilegedCommandProviderRegistry.find(selected)
    }

    private fun commandOutcome(
        providerId: String,
        result: PrivilegedCommandResult,
    ): PointerInputOutcome = when (result) {
        is PrivilegedCommandResult.Failure -> {
            val effect = if (result.effectMayHaveOccurred) {
                EffectCertainty.MAY_HAVE_OCCURRED
            } else {
                EffectCertainty.PROVEN_NO_EFFECT
            }
            val generation = invalidateIfNeeded(providerId, effect)
            PointerInputOutcome(
                providerId = providerId,
                providerState = result.state,
                code = result.code,
                generation = generation,
                effect = effect,
                snapshotInvalidated = effect != EffectCertainty.PROVEN_NO_EFFECT ||
                    result.freshObservationRequired,
                retryable = result.state != CapabilityState.UNSUPPORTED,
                requiredUserStep = result.requiredUserStep,
                freshObservationRequired = result.freshObservationRequired ||
                    effect != EffectCertainty.PROVEN_NO_EFFECT,
                message = result.message,
            )
        }
        is PrivilegedCommandResult.Success -> {
            val receipt = result.receipt
            val receiptCode = receipt["code"]?.jsonPrimitive?.contentOrNull
            val exitCode = receipt["exit_code"]?.jsonPrimitive?.intOrNull
            val timedOut = receipt["timed_out"]?.jsonPrimitive?.booleanOrNull == true
            val cancelled = receipt["cancelled"]?.jsonPrimitive?.booleanOrNull == true
            val processStarted = receipt["process_started"]?.jsonPrimitive?.booleanOrNull == true
            val accepted = receiptCode == "process_exited" &&
                exitCode == 0 &&
                !timedOut &&
                !cancelled
            val effect = when {
                accepted -> EffectCertainty.MAY_HAVE_OCCURRED
                processStarted -> EffectCertainty.MAY_HAVE_OCCURRED
                else -> EffectCertainty.PROVEN_NO_EFFECT
            }
            val generation = invalidateIfNeeded(providerId, effect)
            PointerInputOutcome(
                providerId = providerId,
                providerState = CapabilityState.READY,
                code = if (accepted) "ok" else receiptCode ?: "input_not_dispatched",
                generation = generation,
                effect = effect,
                snapshotInvalidated = effect != EffectCertainty.PROVEN_NO_EFFECT,
                retryable = !accepted,
                freshObservationRequired = effect != EffectCertainty.PROVEN_NO_EFFECT,
                message = if (accepted) null else "The elevated input process did not complete cleanly.",
            )
        }
    }

    private fun invalidateIfNeeded(providerId: String, effect: EffectCertainty): Long =
        if (effect == EffectCertainty.PROVEN_NO_EFFECT) {
            PhoneControlAccessibilityProvider.observationGeneration
        } else {
            PhoneControlAccessibilityProvider.invalidate("elevated_pointer:$providerId")
        }
}

internal suspend fun routePointerInput(
    accessibility: AccessibilityProviderResult<AccessibilityGestureOutcome>,
    observationGeneration: () -> Long,
    elevated: suspend () -> PointerInputOutcome?,
): PointerInputOutcome {
    val primary = accessibility.toPointerInputOutcome(observationGeneration)
    if (
        primary.providerId != ACCESSIBILITY_PROVIDER ||
        primary.effect != EffectCertainty.PROVEN_NO_EFFECT ||
        primary.code !in ELEVATED_FALLBACK_CODES
    ) {
        return primary
    }
    return elevated() ?: primary
}

private fun AccessibilityProviderResult<AccessibilityGestureOutcome>.toPointerInputOutcome(
    observationGeneration: () -> Long,
): PointerInputOutcome = when (this) {
    is AccessibilityProviderResult.Success -> PointerInputOutcome(
        providerId = ACCESSIBILITY_PROVIDER,
        providerState = CapabilityState.READY,
        code = value.code,
        generation = value.generation,
        effect = value.effect,
        snapshotInvalidated = value.snapshotInvalidated,
        freshObservationRequired = value.snapshotInvalidated,
    )
    is AccessibilityProviderResult.Failure -> PointerInputOutcome(
        providerId = ACCESSIBILITY_PROVIDER,
        providerState = when {
            requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
            code == "capability_unavailable" -> CapabilityState.UNAVAILABLE
            else -> CapabilityState.DEGRADED
        },
        code = code,
        generation = observationGeneration(),
        effect = effect,
        snapshotInvalidated = effect != EffectCertainty.PROVEN_NO_EFFECT,
        retryable = retryable,
        requiredUserStep = requiredUserStep,
        freshObservationRequired = freshObservationRequired ||
            effect != EffectCertainty.PROVEN_NO_EFFECT,
        message = message,
    )
}

private fun accessibilityGuardOutcome(
    failure: AccessibilityProviderResult.Failure,
): PointerInputOutcome = PointerInputOutcome(
    providerId = ACCESSIBILITY_PROVIDER,
    providerState = CapabilityState.DEGRADED,
    code = failure.code,
    generation = PhoneControlAccessibilityProvider.observationGeneration,
    effect = failure.effect,
    snapshotInvalidated = failure.effect != EffectCertainty.PROVEN_NO_EFFECT,
    retryable = failure.retryable,
    requiredUserStep = failure.requiredUserStep,
    freshObservationRequired = failure.freshObservationRequired,
    message = failure.message,
)

private fun pointBounds(x: Float, y: Float): TargetBounds {
    val left = floor(x.toDouble()).toInt()
    val top = floor(y.toDouble()).toInt()
    return TargetBounds(left, top, left + 1, top + 1)
}

private fun gestureBounds(fromX: Float, fromY: Float, toX: Float, toY: Float): TargetBounds {
    val left = floor(minOf(fromX, toX).toDouble()).toInt()
    val top = floor(minOf(fromY, toY).toDouble()).toInt()
    val right = ceil(maxOf(fromX, toX).toDouble()).toInt().coerceAtLeast(left + 1)
    val bottom = ceil(maxOf(fromY, toY).toDouble()).toInt().coerceAtLeast(top + 1)
    return TargetBounds(left, top, right, bottom)
}

private const val ACCESSIBILITY_PROVIDER = "accessibility"
private const val POINTER_CAPABILITY = "ui.pointer_action"
private const val INPUT_PROGRAM = "/system/bin/input"
private const val COMMAND_CWD = "/data/local/tmp"
private const val INPUT_TIMEOUT_MS = 5_000L
private val ELEVATED_FALLBACK_CODES = setOf("gesture_rejected", "action_rejected")
