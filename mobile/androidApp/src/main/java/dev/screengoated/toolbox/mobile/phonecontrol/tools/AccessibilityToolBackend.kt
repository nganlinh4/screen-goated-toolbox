package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionVerb
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.PhoneControlVisualProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds

/** A grid is usable only while it still identifies the exact observed frame. */
internal data class AccessibilityGridIdentity(
    val observationGeneration: Long,
    val visualRevision: Long,
    val displayId: Int,
    val windowId: Long,
    val bounds: TargetBounds,
    val columns: Int,
    val rows: Int,
    val surfaceLease: AccessibilitySurfaceLease,
    val rotation: Int,
    val densityDpi: Int,
    val capturedAtMs: Long,
) {
    init {
        require(observationGeneration > 0)
        require(visualRevision > 0)
        require(displayId >= 0)
        require(windowId >= 0)
        require(columns > 0)
        require(rows > 0)
        require(surfaceLease.observationGeneration == observationGeneration)
        require(surfaceLease.displayId == displayId)
        require(surfaceLease.windowId == windowId)
        require(densityDpi > 0)
        require(capturedAtMs >= 0)
    }

    val wireIdentity: String
        get() = listOf(
            observationGeneration,
            visualRevision,
            displayId,
            windowId,
            bounds.left,
            bounds.top,
            bounds.right,
            bounds.bottom,
            surfaceLease.packageOrSurface,
            surfaceLease.windowLayer,
            surfaceLease.authority.wireName,
            surfaceLease.controllerOwned,
            rotation,
            densityDpi,
            capturedAtMs,
            columns,
            rows,
        ).joinToString(":")

    fun matches(observation: AccessibilityObservation): Boolean {
        if (observation.generation != observationGeneration) return false
        return observation.displayRotation == rotation &&
            observation.densityDpi == densityDpi &&
            observation.surfaceLease(displayId, windowId) == surfaceLease &&
            surfaceLease.bounds.contains(bounds)
    }

    fun cellCenter(cell: Int): Pair<Float, Float>? {
        if (cell !in 1..columns * rows) return null
        val column = (cell - 1) % columns
        val row = (cell - 1) / columns
        val width = (bounds.right - bounds.left).toFloat() / columns
        val height = (bounds.bottom - bounds.top).toFloat() / rows
        return bounds.left + width * (column + 0.5f) to
            bounds.top + height * (row + 0.5f)
    }
}

internal data class AccessibilityObservationFrame(
    val observation: AccessibilityObservation,
    val grid: AccessibilityGridIdentity? = null,
)

/** Tool-facing seam keeps Android objects out of tests and future frame providers pluggable. */
internal interface AccessibilityToolBackend {
    val isReady: Boolean
    val observationGeneration: Long

    suspend fun observe(): AccessibilityProviderResult<AccessibilityObservationFrame>

    fun currentTargetIdentity(targetId: Int): PhoneControlTargetIdentity?

    suspend fun act(
        targetId: Int,
        verb: AccessibilityActionVerb,
        value: String?,
        confirmed: Boolean,
    ): AccessibilityProviderResult<AccessibilityActionOutcome>

    suspend fun click(
        lease: AccessibilitySurfaceLease,
        x: Float,
        y: Float,
        expectedVisualRevision: Long?,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome>

    suspend fun swipe(
        lease: AccessibilitySurfaceLease,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        durationMs: Long,
        kind: AccessibilityMutationKind,
        expectedVisualRevision: Long?,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome>
}

internal object AndroidAccessibilityToolBackend : AccessibilityToolBackend {
    override val isReady: Boolean
        get() = PhoneControlAccessibilityProvider.isReady
    override val observationGeneration: Long
        get() = PhoneControlAccessibilityProvider.observationGeneration

    override suspend fun observe(): AccessibilityProviderResult<AccessibilityObservationFrame> =
        when (val result = PhoneControlAccessibilityProvider.observe()) {
            is AccessibilityProviderResult.Failure -> result
            is AccessibilityProviderResult.Success -> AccessibilityProviderResult.Success(
                AccessibilityObservationFrame(
                    observation = result.value,
                    grid = PhoneControlVisualProvider.currentAccessibilityGrid(),
                ),
            )
        }

    override fun currentTargetIdentity(targetId: Int): PhoneControlTargetIdentity? =
        PhoneControlAccessibilityProvider.currentLease(targetId)?.identity

    override suspend fun act(
        targetId: Int,
        verb: AccessibilityActionVerb,
        value: String?,
        confirmed: Boolean,
    ): AccessibilityProviderResult<AccessibilityActionOutcome> =
        PhoneControlAccessibilityProvider.act(targetId, verb, value, confirmed)

    override suspend fun click(
        lease: AccessibilitySurfaceLease,
        x: Float,
        y: Float,
        expectedVisualRevision: Long?,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        PhoneControlAccessibilityProvider.click(
            lease,
            x,
            y,
            expectedVisualRevision = expectedVisualRevision,
        )

    override suspend fun swipe(
        lease: AccessibilitySurfaceLease,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        durationMs: Long,
        kind: AccessibilityMutationKind,
        expectedVisualRevision: Long?,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        PhoneControlAccessibilityProvider.swipe(
            lease,
            fromX,
            fromY,
            toX,
            toY,
            durationMs,
            kind,
            expectedVisualRevision = expectedVisualRevision,
        )
}

private fun TargetBounds.contains(other: TargetBounds): Boolean =
    other.left >= left && other.top >= top && other.right <= right && other.bottom <= bottom
