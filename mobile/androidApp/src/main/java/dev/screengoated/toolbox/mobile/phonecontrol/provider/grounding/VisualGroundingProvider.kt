package dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.PhoneControlVisualProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlScreenPayload
import dev.screengoated.toolbox.mobile.phonecontrol.session.encodePhoneControlScreenImage
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlin.math.max

internal class VisualGroundingProvider(context: Context) {
    private val client = VisualGroundingClient(context)
    private val markLock = Any()
    private val nextMarkId = AtomicInteger(1)
    private var currentMarks: VisualGroundingMarkSet? = null

    val observationGeneration: Long
        get() = PhoneControlVisualProvider.observationGeneration

    suspend fun mapCurrentSurface(
        description: String,
        context: String,
    ): VisualGroundingResult<VisualGroundingMapping> {
        clearMarks()
        val frame = when (val captured = captureFrame()) {
            is VisualGroundingResult.Failure -> return captured
            is VisualGroundingResult.Success -> captured.value
        }
        val started = SystemClock.elapsedRealtime()
        val grounded = when (val result = client.map(description, context, frame.imageBytes)) {
            is GroundingClientResult.Failure -> return result.toProviderFailure()
            is GroundingClientResult.Success -> result.value
        }
        val groundingMs = elapsedSince(started)
        val bitmap = decodeGroundingBitmap(frame.imageBytes)
            ?: return processingFailure("The grounding frame could not be decoded.")
        try {
            val firstId = allocateMarkIds(grounded.size)
            val marks = grounded.mapIndexed { index, coordinate ->
                val point = normalizedPoint(
                    frame,
                    coordinate.x,
                    coordinate.y,
                    coordinate.label,
                    coordinate.modelId,
                )
                val signature = captureVisualTargetSignature(
                    bitmap,
                    frame.identity.cropBounds,
                    point.bounds,
                ) ?: return staleTarget("A grounded target is outside the captured surface.")
                VisualGroundingMark(firstId + index, point, signature)
            }
            val markSet = VisualGroundingMarkSet(frame, marks)
            synchronized(markLock) { currentMarks = markSet }
            val annotated = annotate(bitmap, markSet)
            val annotatedBytes = try {
                encodePhoneControlScreenImage(annotated)
            } finally {
                annotated.recycle()
            }
            VisualGroundingFrameStore.publish(
                frame.identity.observationGeneration,
                buildPhoneControlScreenPayload(annotatedBytes),
            )
            return VisualGroundingResult.Success(
                VisualGroundingMapping(
                    marks = markSet,
                    groundingMs = groundingMs,
                    modelId = grounded.firstOrNull()?.modelId
                        ?: GROUNDING_MODEL_IDS.firstOrNull().orEmpty(),
                ),
            )
        } catch (cancelled: CancellationException) {
            clearMarks()
            throw cancelled
        } finally {
            bitmap.recycle()
        }
    }

    suspend fun locate(
        description: String,
        context: String,
    ): VisualGroundingResult<VisualGroundingVerifiedMark> {
        clearMarks()
        val source = when (val captured = captureFrame()) {
            is VisualGroundingResult.Failure -> return captured
            is VisualGroundingResult.Success -> captured.value
        }
        val groundingStarted = SystemClock.elapsedRealtime()
        val coordinate = when (val result = client.locate(description, context, source.imageBytes)) {
            is GroundingClientResult.Failure -> return result.toProviderFailure()
            is GroundingClientResult.Success -> result.value
        }
        val groundingMs = elapsedSince(groundingStarted)
        return verifyCoordinate(
            source,
            coordinate,
            description,
            context,
            groundingMs,
            allocateMarkIds(1),
        )
            .also { result ->
                if (result is VisualGroundingResult.Success) {
                    synchronized(markLock) {
                        currentMarks = VisualGroundingMarkSet(
                            result.value.frame,
                            listOf(result.value.mark),
                        )
                    }
                }
            }
    }

    suspend fun locateDrag(
        from: String,
        to: String,
        context: String,
    ): VisualGroundingResult<Pair<VisualGroundingVerifiedMark, VisualGroundingVerifiedMark>> {
        clearMarks()
        val source = when (val captured = captureFrame()) {
            is VisualGroundingResult.Failure -> return captured
            is VisualGroundingResult.Success -> captured.value
        }
        val groundingStarted = SystemClock.elapsedRealtime()
        val coordinates = when (val result = client.drag(from, to, context, source.imageBytes)) {
            is GroundingClientResult.Failure -> return result.toProviderFailure()
            is GroundingClientResult.Success -> result.value
        }
        val groundingMs = elapsedSince(groundingStarted)
        val fresh = when (val captured = captureFrame()) {
            is VisualGroundingResult.Failure -> return captured
            is VisualGroundingResult.Success -> captured.value
        }
        if (!sameGroundingSurface(source, fresh)) {
            return staleTarget("The drag surface changed during visual grounding.")
        }
        val fromVerified = verifyOnFrame(
            fresh,
            coordinates.first,
            from,
            context,
            groundingMs,
            allocateMarkIds(2),
        )
        val fromMark = when (fromVerified) {
            is VisualGroundingResult.Failure -> return fromVerified
            is VisualGroundingResult.Success -> fromVerified.value
        }
        val toVerified = verifyOnFrame(
            fresh,
            coordinates.second,
            to,
            context,
            groundingMs,
            fromMark.mark.id + 1,
        )
        val toMark = when (toVerified) {
            is VisualGroundingResult.Failure -> return toVerified
            is VisualGroundingResult.Success -> toVerified.value
        }
        synchronized(markLock) {
            currentMarks = VisualGroundingMarkSet(
                fresh,
                listOf(fromMark.mark, toMark.mark),
            )
        }
        return VisualGroundingResult.Success(fromMark to toMark)
    }

    suspend fun refreshMark(id: Int): VisualGroundingResult<VisualGroundingVerifiedMark> {
        val installed = synchronized(markLock) { currentMarks }
            ?: return staleTarget("There is no current visual mark set.")
        val mark = installed.marks.singleOrNull { it.id == id }
            ?: return staleTarget("The requested mark is not in the current visual frame.")
        val unverified = VisualGroundingVerifiedMark(
            mark = mark,
            frame = installed.frame,
            verificationConfidence = null,
            verificationModelId = null,
            verificationWhat = mark.point.label,
            groundingMs = 0,
            verificationMs = 0,
        )
        return when (val result = revalidateMarks(listOf(unverified))) {
            is VisualGroundingResult.Failure -> result
            is VisualGroundingResult.Success ->
                VisualGroundingResult.Success(result.value.marks.single())
        }
    }

    suspend fun revalidateMarks(
        marks: List<VisualGroundingVerifiedMark>,
    ): VisualGroundingResult<VisualGroundingVerifiedSet> {
        if (marks.isEmpty()) return invalidRequest("At least one visual target is required.")
        val installed = synchronized(markLock) { currentMarks }
            ?: return staleTarget("There is no current visual mark set.")
        if (marks.any { candidate ->
                candidate.frame.wireIdentity != installed.frame.wireIdentity ||
                    installed.marks.none { it.id == candidate.mark.id }
            }
        ) {
            return staleTarget("The verified targets do not belong to the current frame.")
        }
        val started = SystemClock.elapsedRealtime()
        val fresh = when (val captured = captureFrame()) {
            is VisualGroundingResult.Failure -> return captured
            is VisualGroundingResult.Success -> captured.value
        }
        if (!sameGroundingSurface(installed.frame, fresh)) {
            return staleTarget("The visual surface changed before input dispatch.")
        }
        val bitmap = decodeGroundingBitmap(fresh.imageBytes)
            ?: return processingFailure("The fresh visual frame could not be decoded.")
        try {
            val refreshed = marks.map { verified ->
                val signature = captureVisualTargetSignature(
                    bitmap,
                    fresh.identity.cropBounds,
                    verified.mark.point.bounds,
                ) ?: return staleTarget("A verified target is outside the fresh frame.")
                if (!verified.mark.signature.matches(signature)) {
                    return staleTarget("A verified target changed before input dispatch.")
                }
                verified.copy(
                    mark = verified.mark.copy(signature = signature),
                    frame = fresh,
                    pixelRevalidationMs = elapsedSince(started),
                )
            }
            return VisualGroundingResult.Success(
                VisualGroundingVerifiedSet(refreshed, elapsedSince(started)),
            )
        } finally {
            bitmap.recycle()
        }
    }

    fun clearMarks() {
        synchronized(markLock) { currentMarks = null }
        VisualGroundingFrameStore.clear()
    }

    private suspend fun verifyCoordinate(
        source: VisualGroundingFrame,
        coordinate: GroundingCoordinate,
        description: String,
        context: String,
        groundingMs: Long,
        id: Int,
    ): VisualGroundingResult<VisualGroundingVerifiedMark> {
        val fresh = when (val captured = captureFrame()) {
            is VisualGroundingResult.Failure -> return captured
            is VisualGroundingResult.Success -> captured.value
        }
        if (!sameGroundingSurface(source, fresh)) {
            return staleTarget("The visual surface changed during target grounding.")
        }
        return verifyOnFrame(fresh, coordinate, description, context, groundingMs, id)
    }

    private suspend fun verifyOnFrame(
        frame: VisualGroundingFrame,
        coordinate: GroundingCoordinate,
        description: String,
        context: String,
        groundingMs: Long,
        id: Int,
    ): VisualGroundingResult<VisualGroundingVerifiedMark> {
        val point = normalizedPoint(
            frame,
            coordinate.x,
            coordinate.y,
            coordinate.label,
            coordinate.modelId,
        )
        val bitmap = decodeGroundingBitmap(frame.imageBytes)
            ?: return processingFailure("The verification frame could not be decoded.")
        try {
            val signature = captureVisualTargetSignature(
                bitmap,
                frame.identity.cropBounds,
                point.bounds,
            ) ?: return staleTarget("The proposed target is outside the verification frame.")
            val verificationImage = crosshairCrop(bitmap, frame, point)
            val verificationStarted = SystemClock.elapsedRealtime()
            val verification = when (
                val result = client.verify(
                    description,
                    context,
                    verificationImage,
                )
            ) {
                is GroundingClientResult.Failure -> return result.toProviderFailure()
                is GroundingClientResult.Success -> result.value
            }
            return VisualGroundingResult.Success(
                VisualGroundingVerifiedMark(
                    mark = VisualGroundingMark(id, point, signature),
                    frame = frame,
                    verificationConfidence = verification.confidence,
                    verificationModelId = verification.modelId,
                    verificationWhat = verification.what,
                    groundingMs = groundingMs,
                    verificationMs = elapsedSince(verificationStarted),
                ),
            )
        } finally {
            bitmap.recycle()
        }
    }

    private suspend fun captureFrame(): VisualGroundingResult<VisualGroundingFrame> {
        return when (val result = PhoneControlVisualProvider.captureGroundingFrame()) {
            is VisualProviderResult.Failure -> VisualGroundingResult.Failure(
                code = result.code,
                message = result.message,
                retryable = result.retryable,
                requiredUserStep = result.requiredUserStep,
                freshObservationRequired = result.freshObservationRequired,
            )
            is VisualProviderResult.Success -> {
                val lease = result.value.identity.surfaceLease
                    ?: return VisualGroundingResult.Failure(
                        code = "surface_authority_unknown",
                        message = "The current visual frame has no stable input surface.",
                        retryable = true,
                        freshObservationRequired = true,
                    )
                VisualGroundingResult.Success(
                    VisualGroundingFrame(
                        result.value.identity,
                        lease,
                        result.value.imageBytes,
                    ),
                )
            }
        }
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

private fun annotate(source: Bitmap, markSet: VisualGroundingMarkSet): Bitmap {
    val output = requireNotNull(source.copy(Bitmap.Config.ARGB_8888, true)) {
        "Could not allocate grounding annotation frame"
    }
    val canvas = Canvas(output)
    val radius = max(15f, 13f * markSet.frame.identity.densityDpi / 160f)
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
    val bounds = markSet.frame.identity.cropBounds
    markSet.marks.forEach { mark ->
        val x = (mark.point.centerX - bounds.left).toFloat() / (bounds.right - bounds.left) *
            output.width
        val y = (mark.point.centerY - bounds.top).toFloat() / (bounds.bottom - bounds.top) *
            output.height
        canvas.drawCircle(x, y, radius, fill)
        canvas.drawCircle(x, y, radius, outline)
        canvas.drawText(mark.id.toString(), x, y - (text.ascent() + text.descent()) / 2f, text)
    }
    return output
}

private fun crosshairCrop(
    source: Bitmap,
    frame: VisualGroundingFrame,
    point: VisualGroundingPoint,
): ByteArray {
    val bounds = frame.identity.cropBounds
    val centerX = ((point.centerX - bounds.left).toDouble() /
        (bounds.right - bounds.left).coerceAtLeast(1) * source.width).toInt()
    val centerY = ((point.centerY - bounds.top).toDouble() /
        (bounds.bottom - bounds.top).coerceAtLeast(1) * source.height).toInt()
    val cropWidth = max(240, source.width / 4).coerceAtMost(source.width)
    val cropHeight = max(180, source.height / 4).coerceAtMost(source.height)
    val left = (centerX - cropWidth / 2).coerceIn(0, source.width - cropWidth)
    val top = (centerY - cropHeight / 2).coerceIn(0, source.height - cropHeight)
    val extracted = Bitmap.createBitmap(source, left, top, cropWidth, cropHeight)
    val crop = requireNotNull(extracted.copy(Bitmap.Config.ARGB_8888, true)) {
        "Could not allocate grounding verification frame"
    }
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
        if (extracted !== source && extracted !== crop) extracted.recycle()
    }
}

private fun GroundingClientResult.Failure.toProviderFailure() =
    VisualGroundingResult.Failure(
        code,
        message,
        retryable,
        requiredUserStep,
        freshObservationRequired,
    )

private fun processingFailure(message: String) = VisualGroundingResult.Failure(
    code = "visual_grounding_processing_failed",
    message = message,
    retryable = true,
)

private fun staleTarget(message: String) = VisualGroundingResult.Failure(
    code = "stale_target",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)

private fun invalidRequest(message: String) = VisualGroundingResult.Failure(
    code = "invalid_visual_grounding_request",
    message = message,
    retryable = false,
)

private fun elapsedSince(started: Long): Long =
    (SystemClock.elapsedRealtime() - started).coerceAtLeast(0L)
