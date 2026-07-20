package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.view.Display
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.mapCaptureCrop
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlScreenPayload
import dev.screengoated.toolbox.mobile.phonecontrol.session.encodePhoneControlScreenImage
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.max
import kotlinx.coroutines.CancellationException

internal sealed interface UiDetectorProviderResult<out T> {
    data class Success<T>(val value: T) : UiDetectorProviderResult<T>

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean,
        val requiredUserStep: String? = null,
        val freshObservationRequired: Boolean = false,
    ) : UiDetectorProviderResult<Nothing>
}

internal data class UiDetectorMapping(
    val marks: UiDetectorMarkSet,
    val stats: UiDetectorStats,
    val inferenceMs: Long,
    val executionProvider: String,
    val groundingImageBytes: ByteArray,
)

internal data class UiDetectorRefreshedMark(
    val mark: UiDetectorMark,
    val overlap: Float,
    val inferenceMs: Long,
    val observationGeneration: Long,
    val surfaceLease: AccessibilitySurfaceLease,
    val verificationImageBytes: ByteArray,
    val visualRevision: Long = 1L,
)

internal data class UiDetectorRefreshedMarkSet(
    val marks: List<UiDetectorRefreshedMark>,
    val inferenceMs: Long,
    val observationGeneration: Long,
    val surfaceLease: AccessibilitySurfaceLease,
) {
    fun mark(id: Int): UiDetectorRefreshedMark? = marks.singleOrNull { it.mark.id == id }
}

internal object UiDetectorGroundingFrameStore {
    private data class Pending(val generation: Long, val payload: String)
    private val lock = Any()
    private var pending: Pending? = null

    fun publish(generation: Long, payload: String) = synchronized(lock) {
        pending = Pending(generation, payload)
    }

    fun takeForGeneration(generation: Long): String? = synchronized(lock) {
        val frame = pending
        pending = null
        frame?.payload?.takeIf { frame.generation == generation }
    }

    fun clear() = synchronized(lock) {
        pending = null
    }
}

internal class UiDetectorProvider(context: Context) {
    private val modelManager = UiDetectorModelManager.get(context)
    private val markLock = Any()
    private val nextMarkId = AtomicInteger(1)
    private var currentMarks: UiDetectorMarkSet? = null

    suspend fun mapCurrentSurface(): UiDetectorProviderResult<UiDetectorMapping> {
        clearMarks()
        val observation = when (val result = PhoneControlAccessibilityProvider.observe()) {
            is AccessibilityProviderResult.Success -> result.value
            is AccessibilityProviderResult.Failure -> return result.toDetectorFailure()
        }
        val surface = detectorSurface(observation) ?: return UiDetectorProviderResult.Failure(
            "surface_unavailable",
            "No active Android surface could be bound to detector marks.",
            retryable = true,
            freshObservationRequired = true,
        )
        val surfaceLease = surface.surfaceLease(observation.generation)
            ?: return UiDetectorProviderResult.Failure(
                "surface_authority_unknown",
                "The active surface has no stable platform identity for detector marks.",
                retryable = true,
                freshObservationRequired = true,
            )
        if (surface.displayId != Display.DEFAULT_DISPLAY) {
            return UiDetectorProviderResult.Failure(
                "unsupported_display",
                "Accessibility screenshots currently expose only the default display.",
                retryable = false,
            )
        }
        if (!isAccessibilityBlind(observation, surface)) {
            return UiDetectorProviderResult.Failure(
                "structured_surface_available",
                "This surface has usable Accessibility targets; observe and act on their current ids.",
                retryable = false,
            )
        }
        val model = when (val prepared = modelManager.prepare()) {
            is UiDetectorPreparation.Ready -> prepared.model
            is UiDetectorPreparation.Pending -> return UiDetectorProviderResult.Failure(
                code = "capability_unavailable",
                message = prepared.message,
                retryable = true,
                requiredUserStep = prepared.requiredUserStep,
            )
            is UiDetectorPreparation.Failed -> return UiDetectorProviderResult.Failure(
                prepared.code,
                prepared.message,
                prepared.retryable,
            )
        }
        val screenshot = when (
            val result = PhoneControlAccessibilityProvider.screenshot(
                surface.id.toLong(),
                surface.bounds,
            )
        ) {
            is AccessibilityProviderResult.Success -> result.value
            is AccessibilityProviderResult.Failure -> return result.toDetectorFailure()
        }
        try {
            if (screenshot.generation != observation.generation) return staleFrame()
            val cropMapping = mapCaptureCrop(
                surface.bounds,
                screenshot.captureBounds,
                screenshot.bitmap.width,
                screenshot.bitmap.height,
            )
                ?: return UiDetectorProviderResult.Failure(
                    "surface_outside_capture",
                    "The active surface is outside the Accessibility screenshot.",
                    retryable = true,
                    freshObservationRequired = true,
                )
            val crop = screenshot.bitmap.crop(cropMapping.bitmapBounds)
            val inference = try {
                UiDetectorOnnxEngine.detect(
                    crop,
                    cropMapping.absoluteBounds.left,
                    cropMapping.absoluteBounds.top,
                    model,
                )
            } finally {
                if (crop !== screenshot.bitmap) crop.recycle()
            }
            if (PhoneControlAccessibilityProvider.observationGeneration != observation.generation) {
                return staleFrame()
            }
            val accessible = accessibleActionBounds(observation, surface)
            val filtered = inference.output.boxes.filter { box ->
                accessible.none { bounds -> bounds.contains(box.centerX, box.centerY) }
            }
            val selected = selectUiDetectorMarks(filtered, cropMapping.absoluteBounds)
            val firstId = allocateMarkIds(selected.size)
            val frame = UiDetectorFrameIdentity(
                surfaceLease = surfaceLease,
                rotation = observation.displayRotation,
                densityDpi = observation.densityDpi,
                capturedAtMs = screenshot.capturedAtMs,
            )
            val markSet = UiDetectorMarkSet(
                frame,
                selected.mapIndexed { index, box -> UiDetectorMark(firstId + index, box) },
            )
            synchronized(markLock) { currentMarks = markSet }
            val annotated = annotate(
                screenshot.bitmap,
                markSet,
                observation.densityDpi,
                screenshot.captureBounds,
            )
            val groundingImageBytes = try {
                if (PhoneControlAccessibilityProvider.observationGeneration != observation.generation) {
                    clearMarks()
                    return staleFrame()
                }
                val encoded = encodePhoneControlScreenImage(annotated)
                UiDetectorGroundingFrameStore.publish(
                    observation.generation,
                    buildPhoneControlScreenPayload(encoded),
                )
                encoded
            } finally {
                annotated.recycle()
            }
            return UiDetectorProviderResult.Success(
                UiDetectorMapping(
                    marks = markSet,
                    stats = inference.output.stats,
                    inferenceMs = inference.durationMs,
                    executionProvider = inference.executionProvider,
                    groundingImageBytes = groundingImageBytes,
                ),
            )
        } catch (cancelled: CancellationException) {
            clearMarks()
            throw cancelled
        } catch (error: Throwable) {
            clearMarks()
            return UiDetectorProviderResult.Failure(
                code = "detector_inference_failed",
                message = error.message ?: "Local UI detector inference failed.",
                retryable = true,
            )
        } finally {
            screenshot.bitmap.recycle()
        }
    }

    suspend fun refreshMark(id: Int): UiDetectorProviderResult<UiDetectorRefreshedMark> =
        when (val result = refreshMarks(listOf(id))) {
            is UiDetectorProviderResult.Failure -> result
            is UiDetectorProviderResult.Success ->
                UiDetectorProviderResult.Success(result.value.marks.single())
        }

    suspend fun refreshMarks(
        ids: List<Int>,
    ): UiDetectorProviderResult<UiDetectorRefreshedMarkSet> {
        val requestedIds = ids.distinct()
        if (requestedIds.isEmpty()) {
            return UiDetectorProviderResult.Failure(
                code = "invalid_detector_request",
                message = "At least one detector mark is required for refresh.",
                retryable = false,
            )
        }
        val installed = synchronized(markLock) { currentMarks }
            ?: return staleMark("There is no current detector mark set.")
        val installedById = installed.marks.associateBy(UiDetectorMark::id)
        val requested = requestedIds.map { id ->
            installedById[id]
                ?: return staleMark("Mark #$id is not in the current detector frame.")
        }
        val observation = when (val result = PhoneControlAccessibilityProvider.observe()) {
            is AccessibilityProviderResult.Success -> result.value
            is AccessibilityProviderResult.Failure -> return result.toDetectorFailure()
        }
        if (!installed.frame.matches(observation)) return staleMark("The marked surface changed.")
        val prepared = modelManager.prepare()
        val model = (prepared as? UiDetectorPreparation.Ready)?.model
            ?: return preparationFailure(prepared)
        val screenshot = when (
            val result = PhoneControlAccessibilityProvider.screenshot(
                installed.frame.windowId,
                installed.frame.bounds,
            )
        ) {
            is AccessibilityProviderResult.Success -> result.value
            is AccessibilityProviderResult.Failure -> return result.toDetectorFailure()
        }
        try {
            if (screenshot.generation != installed.frame.observationGeneration) {
                return staleMark("The screenshot generation changed.")
            }
            val cropMapping = mapCaptureCrop(
                installed.frame.bounds,
                screenshot.captureBounds,
                screenshot.bitmap.width,
                screenshot.bitmap.height,
            )
                ?: return staleMark("The marked surface is outside the current capture.")
            val crop = screenshot.bitmap.crop(cropMapping.bitmapBounds)
            val inference = try {
                UiDetectorOnnxEngine.detect(
                    crop,
                    cropMapping.absoluteBounds.left,
                    cropMapping.absoluteBounds.top,
                    model,
                )
            } finally {
                if (crop !== screenshot.bitmap) crop.recycle()
            }
            if (PhoneControlAccessibilityProvider.observationGeneration != installed.frame.observationGeneration) {
                return staleMark("The surface changed during mark verification.")
            }
            if (PhoneControlAccessibilityProvider.currentVisualRevision != screenshot.visualRevision) {
                return staleMark("The visual content changed during mark verification.")
            }
            val matches = matchUiDetectorMarks(requested, inference.output.boxes, MIN_REFRESH_IOU)
                ?: return staleMark(
                    "Every requested mark must independently overlap a fresh clickable region.",
                )
            PhoneControlAccessibilityProvider.validateSurfaceMutation(
                lease = installed.frame.surfaceLease,
                kind = AccessibilityMutationKind.POINTER_ACTIVATE,
                confirmed = false,
                affectedBounds = refreshAffectedBounds(matches),
            )?.let { return it.toDetectorFailure() }
            val refreshedMarks = matches.map { match ->
                UiDetectorRefreshedMark(
                    mark = UiDetectorMark(match.requested.id, match.refreshed),
                    overlap = match.overlap,
                    inferenceMs = inference.durationMs,
                    observationGeneration = installed.frame.observationGeneration,
                    surfaceLease = installed.frame.surfaceLease,
                    visualRevision = screenshot.visualRevision,
                    verificationImageBytes = screenshot.captureBounds
                        .mapPointToBitmap(
                            match.refreshed.centerX,
                            match.refreshed.centerY,
                            screenshot.bitmap,
                        )
                        .let { point ->
                            encodeVerificationCrop(screenshot.bitmap, point.first, point.second)
                        },
                )
            }
            return UiDetectorProviderResult.Success(
                UiDetectorRefreshedMarkSet(
                    marks = refreshedMarks,
                    inferenceMs = inference.durationMs,
                    observationGeneration = installed.frame.observationGeneration,
                    surfaceLease = installed.frame.surfaceLease,
                ),
            )
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            return UiDetectorProviderResult.Failure(
                "detector_inference_failed",
                error.message ?: "Could not verify the current detector marks.",
                retryable = true,
                freshObservationRequired = true,
            )
        } finally {
            screenshot.bitmap.recycle()
        }
    }

    fun clearMarks() {
        synchronized(markLock) { currentMarks = null }
        UiDetectorGroundingFrameStore.clear()
    }

    private fun allocateMarkIds(count: Int): Int {
        if (count <= 0) return nextMarkId.get()
        while (true) {
            val current = nextMarkId.get()
            val reset = current > Int.MAX_VALUE - count
            val next = if (reset) count + 1 else current + count
            val first = if (reset) 1 else current
            if (nextMarkId.compareAndSet(current, next)) return first
        }
    }
}

private fun AccessibilityProviderResult.Failure.toDetectorFailure() =
    UiDetectorProviderResult.Failure(
        code,
        message,
        retryable,
        requiredUserStep = requiredUserStep
            ?: if (code == "capability_unavailable") "enable_accessibility" else null,
        freshObservationRequired = freshObservationRequired,
    )

private fun preparationFailure(prepared: UiDetectorPreparation): UiDetectorProviderResult.Failure =
    when (prepared) {
        is UiDetectorPreparation.Pending -> UiDetectorProviderResult.Failure(
            "capability_unavailable",
            prepared.message,
            retryable = true,
            requiredUserStep = prepared.requiredUserStep,
        )
        is UiDetectorPreparation.Failed -> UiDetectorProviderResult.Failure(
            prepared.code,
            prepared.message,
            prepared.retryable,
        )
        is UiDetectorPreparation.Ready -> error("ready preparation cannot fail")
    }

private fun staleFrame() = UiDetectorProviderResult.Failure(
    "stale_frame",
    "The surface changed while detector marks were being created.",
    retryable = true,
    freshObservationRequired = true,
)

private fun staleMark(message: String) = UiDetectorProviderResult.Failure(
    "stale_target",
    message,
    retryable = true,
    freshObservationRequired = true,
)

private fun TargetBounds.contains(x: Int, y: Int): Boolean =
    x in left..right && y in top..bottom

private fun Bitmap.crop(bounds: TargetBounds): Bitmap = Bitmap.createBitmap(
    this,
    bounds.left,
    bounds.top,
    bounds.right - bounds.left,
    bounds.bottom - bounds.top,
)

private fun annotate(
    source: Bitmap,
    marks: UiDetectorMarkSet,
    densityDpi: Int,
    captureBounds: TargetBounds,
): Bitmap {
    val annotated = source.copy(Bitmap.Config.ARGB_8888, true)
        ?: error("Could not allocate detector annotation frame")
    val canvas = Canvas(annotated)
    val radius = max(15f, 13f * densityDpi / 160f)
    val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.rgb(32, 221, 235) }
    val outline = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.BLACK
        style = Paint.Style.STROKE
        strokeWidth = max(2f, radius * 0.14f)
    }
    val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.BLACK
        textAlign = Paint.Align.CENTER
        textSize = radius * 1.05f
        typeface = Typeface.DEFAULT_BOLD
    }
    marks.marks.forEach { mark ->
        val point = captureBounds.mapPointToBitmap(mark.box.centerX, mark.box.centerY, source)
        val x = point.first.toFloat()
        val y = point.second.toFloat()
        canvas.drawCircle(x, y, radius, fill)
        canvas.drawCircle(x, y, radius, outline)
        canvas.drawText(mark.id.toString(), x, y - (text.ascent() + text.descent()) / 2f, text)
    }
    return annotated
}

private fun TargetBounds.mapPointToBitmap(x: Int, y: Int, bitmap: Bitmap): Pair<Int, Int> {
    val width = (right - left).coerceAtLeast(1)
    val height = (bottom - top).coerceAtLeast(1)
    val mappedX = ((x - left).toDouble() * bitmap.width / width).toInt()
        .coerceIn(0, (bitmap.width - 1).coerceAtLeast(0))
    val mappedY = ((y - top).toDouble() * bitmap.height / height).toInt()
        .coerceIn(0, (bitmap.height - 1).coerceAtLeast(0))
    return mappedX to mappedY
}

private fun refreshAffectedBounds(matches: List<UiDetectorRefreshMatch>): TargetBounds {
    if (matches.size == 1) return matches.single().refreshed.bounds
    val boxes = matches.map(UiDetectorRefreshMatch::refreshed)
    return TargetBounds(
        left = boxes.minOf(UiDetectorBox::centerX),
        top = boxes.minOf(UiDetectorBox::centerY),
        right = boxes.maxOf(UiDetectorBox::centerX) + 1,
        bottom = boxes.maxOf(UiDetectorBox::centerY) + 1,
    )
}

private fun encodeVerificationCrop(source: Bitmap, centerX: Int, centerY: Int): ByteArray {
    val cropWidth = max(240, source.width / 4).coerceAtMost(source.width)
    val cropHeight = max(180, source.height / 4).coerceAtMost(source.height)
    val left = (centerX - cropWidth / 2).coerceIn(0, source.width - cropWidth)
    val top = (centerY - cropHeight / 2).coerceIn(0, source.height - cropHeight)
    val extracted = Bitmap.createBitmap(source, left, top, cropWidth, cropHeight)
    val crop = extracted.copy(Bitmap.Config.ARGB_8888, true) ?: run {
        if (extracted !== source) extracted.recycle()
        error("Could not allocate detector verification crop")
    }
    if (extracted !== source && extracted !== crop) extracted.recycle()
    try {
        val x = (centerX - left).toFloat().coerceIn(0f, (crop.width - 1).toFloat())
        val y = (centerY - top).toFloat().coerceIn(0f, (crop.height - 1).toFloat())
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.rgb(255, 32, 32)
            strokeWidth = 3f
        }
        Canvas(crop).apply {
            drawLine(x - 14f, y, x - 4f, y, paint)
            drawLine(x + 4f, y, x + 14f, y, paint)
            drawLine(x, y - 14f, x, y - 4f, paint)
            drawLine(x, y + 4f, x, y + 14f, paint)
        }
        return encodePhoneControlScreenImage(crop)
    } finally {
        crop.recycle()
    }
}

private const val MIN_REFRESH_IOU = 0.35f
