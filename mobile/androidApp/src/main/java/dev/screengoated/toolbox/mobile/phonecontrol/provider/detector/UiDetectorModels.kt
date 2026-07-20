package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlin.math.exp
import kotlin.math.roundToInt

internal object UiDetectorContract {
    const val MODEL_URL =
        "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/ui-detr-1.onnx"
    const val MODEL_BYTES = 131_216_489L
    const val MODEL_SHA256 =
        "1892092320cd55fd182c6afd76ae5bb0fb9695f5fcdf0ba875c1f68d49792ff4"
    const val INPUT_SIDE = 1_024
    const val SCORE_THRESHOLD = 0.70f
    const val DUPLICATE_IOU = 0.92f
    const val MAX_CANDIDATES = 90
    const val DISPLAY_MARKS = 30
    val MEAN = floatArrayOf(0.485f, 0.456f, 0.406f)
    val STD = floatArrayOf(0.229f, 0.224f, 0.225f)
}

internal data class UiDetectorFrameIdentity(
    val surfaceLease: AccessibilitySurfaceLease,
    val rotation: Int,
    val densityDpi: Int,
    val capturedAtMs: Long,
) {
    val observationGeneration: Long
        get() = surfaceLease.observationGeneration
    val displayId: Int
        get() = surfaceLease.displayId
    val windowId: Long
        get() = surfaceLease.windowId
    val packageOrSurface: String
        get() = surfaceLease.packageOrSurface
    val bounds: TargetBounds
        get() = surfaceLease.bounds

    fun matches(observation: AccessibilityObservation): Boolean {
        if (observationGeneration != observation.generation ||
            rotation != observation.displayRotation || densityDpi != observation.densityDpi
        ) {
            return false
        }
        val window = observation.windows.singleOrNull { candidate ->
            candidate.id.toLong() == windowId && candidate.displayId == displayId
        } ?: return false
        return (window.active || window.focused) &&
            window.surfaceLease(observation.generation) == surfaceLease
    }

    val wireIdentity: String
        get() = listOf(
            observationGeneration,
            displayId,
            windowId,
            bounds.left,
            bounds.top,
            bounds.right,
            bounds.bottom,
            packageOrSurface,
            surfaceLease.windowLayer,
            surfaceLease.authority.wireName,
            surfaceLease.controllerOwned,
            rotation,
            densityDpi,
            capturedAtMs,
        ).joinToString(":")
}

internal data class UiDetectorBox(
    val centerX: Int,
    val centerY: Int,
    val score: Float,
    val bounds: TargetBounds,
)

internal data class UiDetectorStats(
    val thresholded: Int,
    val rejectedInvalid: Int,
    val suppressedDuplicates: Int,
    val truncated: Int,
)

internal data class UiDetectorOutput(
    val boxes: List<UiDetectorBox>,
    val stats: UiDetectorStats,
)

internal data class UiDetectorMark(
    val id: Int,
    val box: UiDetectorBox,
)

internal data class UiDetectorMarkSet(
    val frame: UiDetectorFrameIdentity,
    val marks: List<UiDetectorMark>,
)

internal data class UiDetectorRefreshMatch(
    val requested: UiDetectorMark,
    val refreshed: UiDetectorBox,
    val overlap: Float,
)

internal fun detectorSurface(
    observation: AccessibilityObservation,
): AccessibilityWindowSnapshot? = observation.windows
    .filter { it.contentAccessible && !it.controllerOwned && (it.active || it.focused) }
    .sortedWith(compareByDescending<AccessibilityWindowSnapshot> { it.active }.thenByDescending { it.layer })
    .firstOrNull()

internal fun isAccessibilityBlind(
    observation: AccessibilityObservation,
    surface: AccessibilityWindowSnapshot,
): Boolean {
    val actionable = observation.elements.filter { element ->
        element.target.windowId == surface.id.toLong() &&
            element.enabled && element.visible && !element.controllerOwned &&
            element.actions.any(ACTIONABLE_ACTIONS::contains) &&
            listOf(element.label, element.value, element.hint, element.stateDescription, element.viewId)
                .any { !it.isNullOrBlank() }
    }
    if (actionable.isEmpty()) return true
    val viewArea = surface.bounds.area().coerceAtLeast(1L)
    val covered = actionable.sumOf { element -> element.bounds.intersectionArea(surface.bounds) }
    return actionable.size <= 12 && covered.toDouble() / viewArea.toDouble() < 0.03
}

internal fun accessibleActionBounds(
    observation: AccessibilityObservation,
    surface: AccessibilityWindowSnapshot,
): List<TargetBounds> = observation.elements
    .filter { element ->
        element.target.windowId == surface.id.toLong() &&
            element.enabled && element.visible && !element.controllerOwned &&
            element.actions.any(ACTIONABLE_ACTIONS::contains)
    }
    .map { it.bounds }

internal fun postprocessUiDetector(
    detsShape: LongArray,
    dets: FloatArray,
    labelsShape: LongArray,
    labels: FloatArray,
    cropWidth: Int,
    cropHeight: Int,
    originX: Int,
    originY: Int,
): UiDetectorOutput {
    val (queries, classes) = validateOutputShapes(detsShape, dets, labelsShape, labels)
    require(cropWidth > 0 && cropHeight > 0) { "invalid detector crop ${cropWidth}x$cropHeight" }
    val candidates = ArrayList<UiDetectorBox>()
    var thresholded = 0
    var rejected = 0
    repeat(queries) { query ->
        var best = Float.NEGATIVE_INFINITY
        repeat(classes) { classIndex ->
            val value = labels[query * classes + classIndex]
            if (value.isFinite() && value > best) best = value
        }
        if (best == Float.NEGATIVE_INFINITY) {
            rejected += 1
            return@repeat
        }
        val score = sigmoid(best)
        if (score < UiDetectorContract.SCORE_THRESHOLD) return@repeat
        thresholded += 1
        val offset = query * 4
        val bounds = normalizedBounds(dets, offset) ?: run {
            rejected += 1
            return@repeat
        }
        val left = originX + (bounds[0] * cropWidth).roundToInt()
        val top = originY + (bounds[1] * cropHeight).roundToInt()
        val right = originX + (bounds[2] * cropWidth).roundToInt()
        val bottom = originY + (bounds[3] * cropHeight).roundToInt()
        if (right <= left || bottom <= top) {
            rejected += 1
            return@repeat
        }
        candidates += UiDetectorBox(
            centerX = left + (right - left) / 2,
            centerY = top + (bottom - top) / 2,
            score = score,
            bounds = TargetBounds(left, top, right, bottom),
        )
    }
    candidates.sortByDescending(UiDetectorBox::score)
    val deduplicated = ArrayList<UiDetectorBox>()
    var suppressed = 0
    candidates.forEach { candidate ->
        if (deduplicated.any { accepted ->
                detectorIou(candidate.bounds, accepted.bounds) > UiDetectorContract.DUPLICATE_IOU ||
                    sameClickOutcome(candidate, accepted)
            }
        ) {
            suppressed += 1
        } else {
            deduplicated += candidate
        }
    }
    val truncated = (deduplicated.size - UiDetectorContract.MAX_CANDIDATES).coerceAtLeast(0)
    val boxes = deduplicated.take(UiDetectorContract.MAX_CANDIDATES).sortedWith(SPATIAL_ORDER)
    return UiDetectorOutput(
        boxes,
        UiDetectorStats(thresholded, rejected, suppressed, truncated),
    )
}

internal fun selectUiDetectorMarks(
    boxes: List<UiDetectorBox>,
    view: TargetBounds,
    limit: Int = UiDetectorContract.DISPLAY_MARKS,
): List<UiDetectorBox> {
    if (limit <= 0) return emptyList()
    if (boxes.size <= limit) return boxes.sortedWith(SPATIAL_ORDER)
    val confidenceOrder = boxes.sortedByDescending(UiDetectorBox::score)
    val occupied = BooleanArray(BUCKET_COLUMNS * BUCKET_ROWS)
    val selected = BooleanArray(confidenceOrder.size)
    var count = 0
    confidenceOrder.forEachIndexed { index, box ->
        if (count == limit) return@forEachIndexed
        val column = bucket(box.centerX, view.left, view.right - view.left, BUCKET_COLUMNS)
        val row = bucket(box.centerY, view.top, view.bottom - view.top, BUCKET_ROWS)
        val slot = row * BUCKET_COLUMNS + column
        if (!occupied[slot]) {
            occupied[slot] = true
            selected[index] = true
            count += 1
        }
    }
    selected.indices.forEach { index ->
        if (count < limit && !selected[index]) {
            selected[index] = true
            count += 1
        }
    }
    return confidenceOrder.filterIndexed { index, _ -> selected[index] }.sortedWith(SPATIAL_ORDER)
}

internal fun detectorIou(left: TargetBounds, right: TargetBounds): Float {
    val width = (minOf(left.right, right.right) - maxOf(left.left, right.left)).coerceAtLeast(0)
    val height = (minOf(left.bottom, right.bottom) - maxOf(left.top, right.top)).coerceAtLeast(0)
    val intersection = width.toLong() * height.toLong()
    val union = left.area() + right.area() - intersection
    return if (union > 0L) intersection.toFloat() / union.toFloat() else 0f
}

/**
 * Rebinds every requested mark to a distinct fresh detection. A complete
 * thresholded matching is required; ambiguous collapse fails closed instead
 * of turning two independently leased endpoints into one guessed point.
 */
internal fun matchUiDetectorMarks(
    requested: List<UiDetectorMark>,
    candidates: List<UiDetectorBox>,
    minimumOverlap: Float,
): List<UiDetectorRefreshMatch>? {
    if (requested.isEmpty() || candidates.isEmpty()) return null
    val choices = requested.map { mark ->
        candidates.indices
            .map { index -> index to detectorIou(mark.box.bounds, candidates[index].bounds) }
            .filter { (_, overlap) -> overlap >= minimumOverlap }
            .sortedByDescending { it.second }
    }
    if (choices.any { it.isEmpty() }) return null
    val candidateOwner = IntArray(candidates.size) { -1 }
    val requestCandidate = IntArray(requested.size) { -1 }

    fun assign(requestIndex: Int, visited: BooleanArray): Boolean {
        choices[requestIndex].forEach { (candidateIndex, _) ->
            if (visited[candidateIndex]) return@forEach
            visited[candidateIndex] = true
            val owner = candidateOwner[candidateIndex]
            if (owner == -1 || assign(owner, visited)) {
                candidateOwner[candidateIndex] = requestIndex
                requestCandidate[requestIndex] = candidateIndex
                return true
            }
        }
        return false
    }

    choices.indices.sortedBy { choices[it].size }.forEach { requestIndex ->
        if (!assign(requestIndex, BooleanArray(candidates.size))) return null
    }
    return requested.mapIndexed { requestIndex, mark ->
        val candidate = candidates[requestCandidate[requestIndex]]
        UiDetectorRefreshMatch(
            requested = mark,
            refreshed = candidate,
            overlap = detectorIou(mark.box.bounds, candidate.bounds),
        )
    }
}

private fun validateOutputShapes(
    detsShape: LongArray,
    dets: FloatArray,
    labelsShape: LongArray,
    labels: FloatArray,
): Pair<Int, Int> {
    require(detsShape.size == 3 && detsShape[0] == 1L && detsShape[2] == 4L) {
        "unexpected dets shape ${detsShape.contentToString()}; expected [1,N,4]"
    }
    require(labelsShape.size == 3 && labelsShape[0] == 1L) {
        "unexpected labels shape ${labelsShape.contentToString()}; expected [1,N,C]"
    }
    val queries = detsShape[1].toIntExact("dets queries")
    val labelQueries = labelsShape[1].toIntExact("label queries")
    val classes = labelsShape[2].toIntExact("label classes")
    require(queries == labelQueries && classes > 0) { "incompatible detector output shapes" }
    require(dets.size == queries * 4 && labels.size == queries * classes) {
        "detector tensor length mismatch"
    }
    return queries to classes
}

private fun normalizedBounds(values: FloatArray, offset: Int): FloatArray? {
    val cx = values[offset]
    val cy = values[offset + 1]
    val width = values[offset + 2]
    val height = values[offset + 3]
    if (!cx.isFinite() || !cy.isFinite() || !width.isFinite() || !height.isFinite() ||
        width <= 0f || height <= 0f
    ) {
        return null
    }
    val left = (cx - width / 2f).coerceIn(0f, 1f)
    val top = (cy - height / 2f).coerceIn(0f, 1f)
    val right = (cx + width / 2f).coerceIn(0f, 1f)
    val bottom = (cy + height / 2f).coerceIn(0f, 1f)
    return if (right > left && bottom > top) floatArrayOf(left, top, right, bottom) else null
}

private fun sigmoid(value: Float): Float = if (value >= 0f) {
    (1.0 / (1.0 + exp(-value.toDouble()))).toFloat()
} else {
    val exponential = exp(value.toDouble())
    (exponential / (1.0 + exponential)).toFloat()
}

private fun sameClickOutcome(left: UiDetectorBox, right: UiDetectorBox): Boolean {
    val dx = (left.centerX - right.centerX).toLong()
    val dy = (left.centerY - right.centerY).toLong()
    return dx * dx + dy * dy <= 12L * 12L
}

private fun bucket(position: Int, origin: Int, length: Int, count: Int): Int {
    val offset = (position - origin).toLong().coerceIn(0L, (length - 1).coerceAtLeast(0).toLong())
    return ((offset * count) / length.coerceAtLeast(1)).toInt().coerceAtMost(count - 1)
}

private fun TargetBounds.area(): Long =
    (right - left).coerceAtLeast(0).toLong() * (bottom - top).coerceAtLeast(0).toLong()

private fun TargetBounds.intersectionArea(other: TargetBounds): Long =
    (minOf(right, other.right) - maxOf(left, other.left)).coerceAtLeast(0).toLong() *
        (minOf(bottom, other.bottom) - maxOf(top, other.top)).coerceAtLeast(0).toLong()

private fun Long.toIntExact(name: String): Int {
    require(this in 0..Int.MAX_VALUE.toLong()) { "invalid $name: $this" }
    return toInt()
}

private val SPATIAL_ORDER = compareBy<UiDetectorBox>(
    { it.bounds.top },
    { it.bounds.left },
    { it.bounds.bottom },
    { it.bounds.right },
)
private val ACTIONABLE_ACTIONS = setOf("click", "activate", "toggle", "select", "submit")
private const val BUCKET_COLUMNS = 6
private const val BUCKET_ROWS = 4
