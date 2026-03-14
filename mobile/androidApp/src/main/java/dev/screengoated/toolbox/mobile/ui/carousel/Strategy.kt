/*
 * Adapted from androidx.compose.material3.carousel.Strategy
 * Original Copyright 2024 The Android Open Source Project, Apache 2.0
 */
package dev.screengoated.toolbox.mobile.ui.carousel

import androidx.compose.ui.util.fastFilter
import androidx.compose.ui.util.fastForEach
import androidx.compose.ui.util.fastMapIndexed
import androidx.compose.ui.util.lerp
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt

internal class Strategy
private constructor(
    val defaultKeylines: KeylineList,
    val startKeylineSteps: List<KeylineList>,
    val endKeylineSteps: List<KeylineList>,
    val availableSpace: Float,
    val itemSpacing: Float,
    val beforeContentPadding: Float,
    val afterContentPadding: Float,
) {
    constructor(
        defaultKeylines: KeylineList,
        availableSpace: Float,
        itemSpacing: Float,
        beforeContentPadding: Float,
        afterContentPadding: Float,
    ) : this(
        defaultKeylines = defaultKeylines,
        startKeylineSteps =
            getStartKeylineSteps(
                defaultKeylines,
                availableSpace,
                itemSpacing,
                beforeContentPadding,
            ),
        endKeylineSteps =
            getEndKeylineSteps(defaultKeylines, availableSpace, itemSpacing, afterContentPadding),
        availableSpace = availableSpace,
        itemSpacing = itemSpacing,
        beforeContentPadding = beforeContentPadding,
        afterContentPadding = afterContentPadding,
    )

    private val startShiftDistance = getStartShiftDistance(startKeylineSteps, beforeContentPadding)
    private val endShiftDistance = getEndShiftDistance(endKeylineSteps, afterContentPadding)
    private val startShiftPoints =
        getStepInterpolationPoints(startShiftDistance, startKeylineSteps, true)
    private val endShiftPoints =
        getStepInterpolationPoints(endShiftDistance, endKeylineSteps, false)

    val itemMainAxisSize: Float
        get() = defaultKeylines.firstFocal.size

    val isValid: Boolean =
        defaultKeylines.isNotEmpty() && availableSpace != 0f && itemMainAxisSize != 0f

    private var lastStartAndEndKeylineListSteps: List<KeylineList>? = null

    internal fun getKeylineListForScrollOffset(
        scrollOffset: Float,
        maxScrollOffset: Float,
        roundToNearestStep: Boolean = false,
    ): KeylineList {
        val positiveScrollOffset = max(0f, scrollOffset)
        val startShiftOffset = startShiftDistance
        val endShiftOffset = max(0f, maxScrollOffset - endShiftDistance)

        if (positiveScrollOffset in startShiftOffset..endShiftOffset) {
            return defaultKeylines
        }

        var interpolation =
            lerp(
                outputMin = 1f,
                outputMax = 0f,
                inputMin = 0f,
                inputMax = startShiftOffset,
                value = positiveScrollOffset,
            )
        var shiftPoints = startShiftPoints
        var steps = startKeylineSteps

        if (positiveScrollOffset > endShiftOffset) {
            interpolation =
                lerp(
                    outputMin = 0f,
                    outputMax = 1f,
                    inputMin = endShiftOffset,
                    inputMax = maxScrollOffset,
                    value = positiveScrollOffset,
                )
            shiftPoints = endShiftPoints
            steps = endKeylineSteps

            if (
                endShiftOffset < 0.01f && startKeylineSteps.size == 2 && endKeylineSteps.size == 2
            ) {
                if (lastStartAndEndKeylineListSteps == null) {
                    lastStartAndEndKeylineListSteps =
                        listOf(startKeylineSteps.last(), endKeylineSteps.last())
                }
                steps = lastStartAndEndKeylineListSteps!!
            }
        }

        val shiftPointRange = getShiftPointRange(steps.size, shiftPoints, interpolation)

        if (roundToNearestStep) {
            val roundedStepIndex =
                if (shiftPointRange.steppedInterpolation.roundToInt() == 0) {
                    shiftPointRange.fromStepIndex
                } else {
                    shiftPointRange.toStepIndex
                }
            return steps[roundedStepIndex]
        }

        return lerp(
            steps[shiftPointRange.fromStepIndex],
            steps[shiftPointRange.toStepIndex],
            shiftPointRange.steppedInterpolation,
        )
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is Strategy) return false
        if (!isValid && !other.isValid) return true
        if (isValid != other.isValid) return false
        if (availableSpace != other.availableSpace) return false
        if (itemSpacing != other.itemSpacing) return false
        if (beforeContentPadding != other.beforeContentPadding) return false
        if (afterContentPadding != other.afterContentPadding) return false
        if (itemMainAxisSize != other.itemMainAxisSize) return false
        if (startShiftDistance != other.startShiftDistance) return false
        if (endShiftDistance != other.endShiftDistance) return false
        if (startShiftPoints != other.startShiftPoints) return false
        if (endShiftPoints != other.endShiftPoints) return false
        if (defaultKeylines != other.defaultKeylines) return false
        return true
    }

    override fun hashCode(): Int {
        if (!isValid) return isValid.hashCode()
        var result = isValid.hashCode()
        result = 31 * result + availableSpace.hashCode()
        result = 31 * result + itemSpacing.hashCode()
        result = 31 * result + beforeContentPadding.hashCode()
        result = 31 * result + afterContentPadding.hashCode()
        result = 31 * result + itemMainAxisSize.hashCode()
        result = 31 * result + startShiftDistance.hashCode()
        result = 31 * result + endShiftDistance.hashCode()
        result = 31 * result + startShiftPoints.hashCode()
        result = 31 * result + endShiftPoints.hashCode()
        result = 31 * result + defaultKeylines.hashCode()
        return result
    }

    companion object {
        val Empty =
            Strategy(
                defaultKeylines = emptyKeylineList(),
                startKeylineSteps = emptyList(),
                endKeylineSteps = emptyList(),
                availableSpace = 0f,
                itemSpacing = 0f,
                beforeContentPadding = 0f,
                afterContentPadding = 0f,
            )
    }
}

private fun getStartShiftDistance(
    startKeylineSteps: List<KeylineList>,
    beforeContentPadding: Float,
): Float {
    if (startKeylineSteps.isEmpty()) return 0f
    return max(
        startKeylineSteps.last().first().unadjustedOffset -
            startKeylineSteps.first().first().unadjustedOffset,
        beforeContentPadding,
    )
}

private fun getEndShiftDistance(
    endKeylineSteps: List<KeylineList>,
    afterContentPadding: Float,
): Float {
    if (endKeylineSteps.isEmpty()) return 0f
    return max(
        endKeylineSteps.first().last().unadjustedOffset -
            endKeylineSteps.last().last().unadjustedOffset,
        afterContentPadding,
    )
}

private fun getStartKeylineSteps(
    defaultKeylines: KeylineList,
    carouselMainAxisSize: Float,
    itemSpacing: Float,
    beforeContentPadding: Float,
): List<KeylineList> {
    if (defaultKeylines.isEmpty()) return emptyList()
    val steps: MutableList<KeylineList> = mutableListOf()
    steps.add(defaultKeylines)
    if (defaultKeylines.isFirstFocalItemAtStartOfContainer()) {
        if (beforeContentPadding != 0f) {
            steps.add(
                createShiftedKeylineListForContentPadding(
                    defaultKeylines,
                    carouselMainAxisSize,
                    itemSpacing,
                    beforeContentPadding,
                    defaultKeylines.firstFocal,
                    defaultKeylines.firstFocalIndex,
                )
            )
        }
        return steps
    }
    val startIndex = defaultKeylines.firstNonAnchorIndex
    val endIndex = defaultKeylines.firstFocalIndex
    val numberOfSteps = endIndex - startIndex
    if (numberOfSteps <= 0 && defaultKeylines.firstFocal.cutoff > 0) {
        steps.add(
            moveKeylineAndCreateShiftedKeylineList(
                from = defaultKeylines,
                srcIndex = 0,
                dstIndex = 0,
                carouselMainAxisSize = carouselMainAxisSize,
                itemSpacing = itemSpacing,
            )
        )
        return steps
    }
    var i = 0
    while (i < numberOfSteps) {
        val prevStep = steps.last()
        val originalItemIndex = startIndex + i
        var dstIndex = defaultKeylines.lastIndex
        if (originalItemIndex > 0) {
            val originalNeighborBeforeSize = defaultKeylines[originalItemIndex - 1].size
            dstIndex = prevStep.firstIndexAfterFocalRangeWithSize(originalNeighborBeforeSize) - 1
        }
        steps.add(
            moveKeylineAndCreateShiftedKeylineList(
                from = prevStep,
                srcIndex = defaultKeylines.firstNonAnchorIndex,
                dstIndex = dstIndex,
                carouselMainAxisSize = carouselMainAxisSize,
                itemSpacing = itemSpacing,
            )
        )
        i++
    }
    if (beforeContentPadding != 0f) {
        steps[steps.lastIndex] =
            createShiftedKeylineListForContentPadding(
                steps.last(),
                carouselMainAxisSize,
                itemSpacing,
                beforeContentPadding,
                steps.last().firstFocal,
                steps.last().firstFocalIndex,
            )
    }
    return steps
}

private fun getEndKeylineSteps(
    defaultKeylines: KeylineList,
    carouselMainAxisSize: Float,
    itemSpacing: Float,
    afterContentPadding: Float,
): List<KeylineList> {
    if (defaultKeylines.isEmpty()) return emptyList()
    val steps: MutableList<KeylineList> = mutableListOf()
    steps.add(defaultKeylines)
    if (defaultKeylines.isLastFocalItemAtEndOfContainer(carouselMainAxisSize)) {
        if (afterContentPadding != 0f) {
            steps.add(
                createShiftedKeylineListForContentPadding(
                    defaultKeylines,
                    carouselMainAxisSize,
                    itemSpacing,
                    -afterContentPadding,
                    defaultKeylines.lastFocal,
                    defaultKeylines.lastFocalIndex,
                )
            )
        }
        return steps
    }
    val startIndex = defaultKeylines.lastFocalIndex
    val endIndex = defaultKeylines.lastNonAnchorIndex
    val numberOfSteps = endIndex - startIndex
    if (numberOfSteps <= 0 && defaultKeylines.lastFocal.cutoff > 0) {
        steps.add(
            moveKeylineAndCreateShiftedKeylineList(
                from = defaultKeylines,
                srcIndex = 0,
                dstIndex = 0,
                carouselMainAxisSize = carouselMainAxisSize,
                itemSpacing = itemSpacing,
            )
        )
        return steps
    }
    var i = 0
    while (i < numberOfSteps) {
        val prevStep = steps.last()
        val originalItemIndex = endIndex - i
        var dstIndex = 0
        if (originalItemIndex < defaultKeylines.lastIndex) {
            val originalNeighborAfterSize = defaultKeylines[originalItemIndex + 1].size
            dstIndex = prevStep.lastIndexBeforeFocalRangeWithSize(originalNeighborAfterSize) + 1
        }
        val keylines =
            moveKeylineAndCreateShiftedKeylineList(
                from = prevStep,
                srcIndex = defaultKeylines.lastNonAnchorIndex,
                dstIndex = dstIndex,
                carouselMainAxisSize = carouselMainAxisSize,
                itemSpacing = itemSpacing,
            )
        steps.add(keylines)
        i++
    }
    if (afterContentPadding != 0f) {
        steps[steps.lastIndex] =
            createShiftedKeylineListForContentPadding(
                steps.last(),
                carouselMainAxisSize,
                itemSpacing,
                -afterContentPadding,
                steps.last().lastFocal,
                steps.last().lastFocalIndex,
            )
    }
    return steps
}

private fun createShiftedKeylineListForContentPadding(
    from: KeylineList,
    carouselMainAxisSize: Float,
    itemSpacing: Float,
    contentPadding: Float,
    pivot: Keyline,
    pivotIndex: Int,
): KeylineList {
    val numberOfNonAnchorKeylines = from.fastFilter { !it.isAnchor }.count()
    val sizeReduction = contentPadding / numberOfNonAnchorKeylines
    val newKeylines =
        keylineListOf(
            carouselMainAxisSize = carouselMainAxisSize,
            itemSpacing = itemSpacing,
            pivotIndex = pivotIndex,
            pivotOffset = pivot.offset - (sizeReduction / 2f) + contentPadding,
        ) {
            from.fastForEach { k -> add(k.size - abs(sizeReduction), k.isAnchor) }
        }
    return KeylineList(
        newKeylines.fastMapIndexed { i, k -> k.copy(unadjustedOffset = from[i].unadjustedOffset) }
    )
}

private fun moveKeylineAndCreateShiftedKeylineList(
    from: KeylineList,
    srcIndex: Int,
    dstIndex: Int,
    carouselMainAxisSize: Float,
    itemSpacing: Float,
): KeylineList {
    val pivotDir = if (srcIndex > dstIndex) 1 else -1
    val pivotDelta = (from[srcIndex].size - from[srcIndex].cutoff + itemSpacing) * pivotDir
    val newPivotIndex = from.pivotIndex + pivotDir
    val newPivotOffset = from.pivot.offset + pivotDelta
    return keylineListOf(carouselMainAxisSize, itemSpacing, newPivotIndex, newPivotOffset) {
        from.toMutableList().move(srcIndex, dstIndex).fastForEach { k -> add(k.size, k.isAnchor) }
    }
}

private fun getStepInterpolationPoints(
    totalShiftDistance: Float,
    steps: List<KeylineList>,
    isShiftingLeft: Boolean,
): List<Float> {
    val points = mutableListOf(0f)
    if (totalShiftDistance == 0f || steps.isEmpty()) {
        return points
    }
    (1 until steps.size).map { i ->
        val prevKeylines = steps[i - 1]
        val currKeylines = steps[i]
        val distanceShifted =
            if (isShiftingLeft) {
                currKeylines.first().unadjustedOffset - prevKeylines.first().unadjustedOffset
            } else {
                prevKeylines.last().unadjustedOffset - currKeylines.last().unadjustedOffset
            }
        val stepPercentage = distanceShifted / totalShiftDistance
        val point = if (i == steps.lastIndex) 1f else points[i - 1] + stepPercentage
        points.add(point)
    }
    return points
}

private data class ShiftPointRange(
    val fromStepIndex: Int,
    val toStepIndex: Int,
    val steppedInterpolation: Float,
)

private fun getShiftPointRange(
    stepsCount: Int,
    shiftPoint: List<Float>,
    interpolation: Float,
): ShiftPointRange {
    var lowerBounds = shiftPoint[0]
    (1 until stepsCount).forEach { i ->
        val upperBounds = shiftPoint[i]
        if (interpolation <= upperBounds) {
            return ShiftPointRange(
                fromStepIndex = i - 1,
                toStepIndex = i,
                steppedInterpolation = lerp(0f, 1f, lowerBounds, upperBounds, interpolation),
            )
        }
        lowerBounds = upperBounds
    }
    return ShiftPointRange(fromStepIndex = 0, toStepIndex = 0, steppedInterpolation = 0f)
}

private fun MutableList<Keyline>.move(srcIndex: Int, dstIndex: Int): MutableList<Keyline> {
    val keyline = get(srcIndex)
    removeAt(srcIndex)
    add(dstIndex, keyline)
    return this
}

internal fun lerp(
    outputMin: Float,
    outputMax: Float,
    inputMin: Float,
    inputMax: Float,
    value: Float,
): Float {
    if (value <= inputMin) {
        return outputMin
    } else if (value >= inputMax) {
        return outputMax
    }
    return lerp(outputMin, outputMax, (value - inputMin) / (inputMax - inputMin))
}
