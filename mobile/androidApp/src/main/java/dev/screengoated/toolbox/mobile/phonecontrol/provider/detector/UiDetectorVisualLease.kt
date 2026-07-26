package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import android.graphics.Bitmap
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlinx.coroutines.CancellationException

internal class UiDetectorVisualSignature(
    private val rgb: ByteArray,
) {
    init {
        require(rgb.size == SIGNATURE_SIDE * SIGNATURE_SIDE * RGB_CHANNELS) {
            "invalid UI detector visual signature"
        }
    }

    fun matches(other: UiDetectorVisualSignature): Boolean {
        var totalDelta = 0L
        var changedSamples = 0
        var offset = 0
        while (offset < rgb.size) {
            val delta = (
                abs(rgb[offset].unsigned() - other.rgb[offset].unsigned()) +
                    abs(rgb[offset + 1].unsigned() - other.rgb[offset + 1].unsigned()) +
                    abs(rgb[offset + 2].unsigned() - other.rgb[offset + 2].unsigned())
                ) / RGB_CHANNELS
            totalDelta += delta
            if (delta >= LARGE_SAMPLE_DELTA) changedSamples += 1
            offset += RGB_CHANNELS
        }
        val samples = rgb.size / RGB_CHANNELS
        return totalDelta <= samples.toLong() * MAX_MEAN_SAMPLE_DELTA &&
            changedSamples * 100 <= samples * MAX_CHANGED_SAMPLE_PERCENT
    }
}

internal fun captureUiDetectorVisualSignature(
    source: Bitmap,
    captureBounds: TargetBounds,
    targetBounds: TargetBounds,
): UiDetectorVisualSignature? {
    val mapped = captureBounds.mapBoundsToBitmap(targetBounds, source) ?: return null
    val extracted = Bitmap.createBitmap(
        source,
        mapped.left,
        mapped.top,
        mapped.right - mapped.left,
        mapped.bottom - mapped.top,
    )
    val scaled = Bitmap.createScaledBitmap(
        extracted,
        SIGNATURE_SIDE,
        SIGNATURE_SIDE,
        true,
    )
    try {
        val pixels = IntArray(SIGNATURE_SIDE * SIGNATURE_SIDE)
        scaled.getPixels(pixels, 0, SIGNATURE_SIDE, 0, 0, SIGNATURE_SIDE, SIGNATURE_SIDE)
        val rgb = ByteArray(pixels.size * RGB_CHANNELS)
        pixels.forEachIndexed { index, pixel ->
            val offset = index * RGB_CHANNELS
            rgb[offset] = ((pixel ushr 16) and 0xff).toByte()
            rgb[offset + 1] = ((pixel ushr 8) and 0xff).toByte()
            rgb[offset + 2] = (pixel and 0xff).toByte()
        }
        return UiDetectorVisualSignature(rgb)
    } finally {
        if (scaled !== extracted) scaled.recycle()
        if (extracted !== source) extracted.recycle()
    }
}

internal suspend fun revalidateUiDetectorVisualLease(
    frame: UiDetectorFrameIdentity,
    marks: List<UiDetectorRefreshedMark>,
): UiDetectorProviderResult<UiDetectorRefreshedMarkSet> {
    val startedAtMs = SystemClock.elapsedRealtime()
    val observation = when (val result = PhoneControlAccessibilityProvider.observe()) {
        is AccessibilityProviderResult.Success -> result.value
        is AccessibilityProviderResult.Failure -> return result.toVisualLeaseFailure()
    }
    if (!frame.matches(observation)) {
        return visualLeaseStaleTarget("The verified detector surface changed.")
    }
    val screenshot = when (
        val result = PhoneControlAccessibilityProvider.screenshot(
            frame.windowId,
            frame.bounds,
        )
    ) {
        is AccessibilityProviderResult.Success -> result.value
        is AccessibilityProviderResult.Failure -> return result.toVisualLeaseFailure()
    }
    try {
        if (screenshot.generation != frame.observationGeneration) {
            return visualLeaseStaleTarget("The target-local screenshot generation changed.")
        }
        val signatures = marks.map { mark ->
            captureUiDetectorVisualSignature(
                screenshot.bitmap,
                screenshot.captureBounds,
                mark.mark.box.bounds,
            ) ?: return visualLeaseStaleTarget(
                "A verified target is outside the current capture.",
            )
        }
        if (marks.indices.any { !marks[it].visualSignature.matches(signatures[it]) }) {
            return visualLeaseStaleTarget(
                "A verified target changed during semantic verification.",
            )
        }
        if (PhoneControlAccessibilityProvider.currentVisualRevision != screenshot.visualRevision) {
            return UiDetectorProviderResult.Failure(
                code = "stale_frame",
                message = "The visual revision changed during final target validation.",
                retryable = true,
                freshObservationRequired = true,
            )
        }
        PhoneControlAccessibilityProvider.validateSurfaceMutation(
            lease = frame.surfaceLease,
            kind = AccessibilityMutationKind.POINTER_ACTIVATE,
            confirmed = false,
            affectedBounds = marks.unionBounds(),
        )?.let { return it.toVisualLeaseFailure() }
        val elapsedMs = (SystemClock.elapsedRealtime() - startedAtMs).coerceAtLeast(0L)
        val refreshed = marks.mapIndexed { index, mark ->
            mark.copy(
                visualSignature = signatures[index],
                visualRevision = screenshot.visualRevision,
                pixelRevalidationMs = elapsedMs,
            )
        }
        return UiDetectorProviderResult.Success(
            UiDetectorRefreshedMarkSet(
                marks = refreshed,
                inferenceMs = marks.maxOf(UiDetectorRefreshedMark::inferenceMs),
                observationGeneration = frame.observationGeneration,
                surfaceLease = frame.surfaceLease,
                pixelRevalidationMs = elapsedMs,
            ),
        )
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (error: Throwable) {
        return UiDetectorProviderResult.Failure(
            code = "target_revalidation_failed",
            message = error.message ?: "Could not renew the target-local visual lease.",
            retryable = true,
            freshObservationRequired = true,
        )
    } finally {
        screenshot.bitmap.recycle()
    }
}

private fun TargetBounds.mapBoundsToBitmap(
    target: TargetBounds,
    bitmap: Bitmap,
): TargetBounds? {
    val surfaceWidth = (right - left).coerceAtLeast(1)
    val surfaceHeight = (bottom - top).coerceAtLeast(1)
    val mappedLeft = floor(
        (target.left - left).toDouble() * bitmap.width / surfaceWidth,
    ).toInt().coerceIn(0, bitmap.width)
    val mappedTop = floor(
        (target.top - top).toDouble() * bitmap.height / surfaceHeight,
    ).toInt().coerceIn(0, bitmap.height)
    val mappedRight = ceil(
        (target.right - left).toDouble() * bitmap.width / surfaceWidth,
    ).toInt().coerceIn(0, bitmap.width)
    val mappedBottom = ceil(
        (target.bottom - top).toDouble() * bitmap.height / surfaceHeight,
    ).toInt().coerceIn(0, bitmap.height)
    return TargetBounds(mappedLeft, mappedTop, mappedRight, mappedBottom)
        .takeIf { it.right > it.left && it.bottom > it.top }
}

private fun Byte.unsigned(): Int = toInt() and 0xff

private fun AccessibilityProviderResult.Failure.toVisualLeaseFailure() =
    UiDetectorProviderResult.Failure(
        code = code,
        message = message,
        retryable = retryable,
        requiredUserStep = requiredUserStep
            ?: if (code == "capability_unavailable") "enable_accessibility" else null,
        freshObservationRequired = freshObservationRequired,
    )

private fun visualLeaseStaleTarget(message: String) = UiDetectorProviderResult.Failure(
    code = "stale_target",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)

private fun List<UiDetectorRefreshedMark>.unionBounds(): TargetBounds {
    val boxes = map { it.mark.box.bounds }
    return TargetBounds(
        left = boxes.minOf(TargetBounds::left),
        top = boxes.minOf(TargetBounds::top),
        right = boxes.maxOf(TargetBounds::right),
        bottom = boxes.maxOf(TargetBounds::bottom),
    )
}

private const val SIGNATURE_SIDE = 16
private const val RGB_CHANNELS = 3
private const val MAX_MEAN_SAMPLE_DELTA = 18
private const val LARGE_SAMPLE_DELTA = 48
private const val MAX_CHANGED_SAMPLE_PERCENT = 12
