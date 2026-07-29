package dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualFrameIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.abs
import kotlin.math.roundToInt

internal sealed interface VisualGroundingResult<out T> {
    data class Success<T>(val value: T) : VisualGroundingResult<T>

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean,
        val requiredUserStep: String? = null,
        val freshObservationRequired: Boolean = false,
    ) : VisualGroundingResult<Nothing>
}

internal data class VisualGroundingFrame(
    val identity: VisualFrameIdentity,
    val lease: AccessibilitySurfaceLease,
    val imageBytes: ByteArray,
) {
    val wireIdentity: String
        get() = identity.wireIdentity
}

internal data class VisualGroundingPoint(
    val centerX: Int,
    val centerY: Int,
    val bounds: TargetBounds,
    val label: String,
    val modelId: String,
)

internal data class VisualGroundingMark(
    val id: Int,
    val point: VisualGroundingPoint,
    val signature: VisualTargetSignature,
)

internal data class VisualGroundingMarkSet(
    val frame: VisualGroundingFrame,
    val marks: List<VisualGroundingMark>,
)

internal data class VisualGroundingMapping(
    val marks: VisualGroundingMarkSet,
    val groundingMs: Long,
    val modelId: String,
)

internal data class VisualGroundingVerifiedMark(
    val mark: VisualGroundingMark,
    val frame: VisualGroundingFrame,
    val verificationConfidence: Int?,
    val verificationModelId: String?,
    val verificationWhat: String?,
    val groundingMs: Long,
    val verificationMs: Long,
    val pixelRevalidationMs: Long = 0,
)

internal data class VisualGroundingVerifiedSet(
    val marks: List<VisualGroundingVerifiedMark>,
    val pixelRevalidationMs: Long,
) {
    fun mark(id: Int): VisualGroundingVerifiedMark? = marks.singleOrNull { it.mark.id == id }
}

internal class VisualTargetSignature(
    private val rgb: ByteArray,
) {
    init {
        require(rgb.size == SIGNATURE_SIDE * SIGNATURE_SIDE * RGB_CHANNELS)
    }

    fun matches(other: VisualTargetSignature): Boolean {
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

internal fun decodeGroundingBitmap(imageBytes: ByteArray): Bitmap? =
    BitmapFactory.decodeByteArray(imageBytes, 0, imageBytes.size)

internal fun captureVisualTargetSignature(
    source: Bitmap,
    captureBounds: TargetBounds,
    targetBounds: TargetBounds,
): VisualTargetSignature? {
    val mapped = captureBounds.mapBoundsToBitmap(targetBounds, source) ?: return null
    val extracted = Bitmap.createBitmap(
        source,
        mapped.left,
        mapped.top,
        mapped.right - mapped.left,
        mapped.bottom - mapped.top,
    )
    val scaled = Bitmap.createScaledBitmap(extracted, SIGNATURE_SIDE, SIGNATURE_SIDE, true)
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
        return VisualTargetSignature(rgb)
    } finally {
        if (scaled !== extracted && scaled !== source) scaled.recycle()
        if (extracted !== source) extracted.recycle()
    }
}

internal fun normalizedPoint(
    frame: VisualGroundingFrame,
    x: Int,
    y: Int,
    label: String,
    modelId: String,
): VisualGroundingPoint {
    val capture = frame.identity.cropBounds
    val width = capture.right - capture.left
    val height = capture.bottom - capture.top
    val centerX = capture.left + (x.coerceIn(0, 1000) / 1000.0 * width)
        .roundToInt().coerceIn(0, (width - 1).coerceAtLeast(0))
    val centerY = capture.top + (y.coerceIn(0, 1000) / 1000.0 * height)
        .roundToInt().coerceIn(0, (height - 1).coerceAtLeast(0))
    val halfWidth = (width * TARGET_SIGNATURE_FRACTION).roundToInt().coerceAtLeast(1)
    val halfHeight = (height * TARGET_SIGNATURE_FRACTION).roundToInt().coerceAtLeast(1)
    return VisualGroundingPoint(
        centerX = centerX,
        centerY = centerY,
        bounds = TargetBounds(
            (centerX - halfWidth).coerceAtLeast(capture.left),
            (centerY - halfHeight).coerceAtLeast(capture.top),
            (centerX + halfWidth + 1).coerceAtMost(capture.right),
            (centerY + halfHeight + 1).coerceAtMost(capture.bottom),
        ),
        label = label,
        modelId = modelId,
    )
}

internal fun sameGroundingSurface(
    left: VisualGroundingFrame,
    right: VisualGroundingFrame,
): Boolean = left.lease == right.lease &&
    left.identity.cropBounds == right.identity.cropBounds &&
    left.identity.rotation == right.identity.rotation &&
    left.identity.densityDpi == right.identity.densityDpi &&
    left.identity.captureProvider == right.identity.captureProvider

internal object VisualGroundingFrameStore {
    private val current = AtomicReference<Pair<Long, String>?>(null)

    fun publish(generation: Long, payload: String) {
        current.set(generation to payload)
    }

    fun take(generation: Long): String? =
        current.getAndSet(null)?.takeIf { it.first == generation }?.second

    fun clear() {
        current.set(null)
    }
}

private fun TargetBounds.mapBoundsToBitmap(target: TargetBounds, source: Bitmap): TargetBounds? {
    val captureWidth = right - left
    val captureHeight = bottom - top
    if (captureWidth <= 0 || captureHeight <= 0) return null
    val mapped = TargetBounds(
        ((target.left - left).toDouble() * source.width / captureWidth).roundToInt(),
        ((target.top - top).toDouble() * source.height / captureHeight).roundToInt(),
        ((target.right - left).toDouble() * source.width / captureWidth).roundToInt(),
        ((target.bottom - top).toDouble() * source.height / captureHeight).roundToInt(),
    )
    val clipped = TargetBounds(
        mapped.left.coerceIn(0, source.width),
        mapped.top.coerceIn(0, source.height),
        mapped.right.coerceIn(0, source.width),
        mapped.bottom.coerceIn(0, source.height),
    )
    return clipped.takeIf { it.right > it.left && it.bottom > it.top }
}

private fun Byte.unsigned(): Int = toInt() and 0xff

private const val SIGNATURE_SIDE = 24
private const val RGB_CHANNELS = 3
private const val LARGE_SAMPLE_DELTA = 52
private const val MAX_MEAN_SAMPLE_DELTA = 30L
private const val MAX_CHANGED_SAMPLE_PERCENT = 28
private const val TARGET_SIGNATURE_FRACTION = 0.035
