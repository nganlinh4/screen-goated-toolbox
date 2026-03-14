/*
 * Adapted from androidx.compose.material3.carousel.Keylines
 * Original Copyright 2024 The Android Open Source Project, Apache 2.0
 */
package dev.screengoated.toolbox.mobile.ui.carousel

import kotlin.jvm.JvmInline
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

/** Mirrors [androidx.compose.material3.carousel.CarouselAlignment]. */
@JvmInline
internal value class CarouselAlignment private constructor(internal val value: Int) {
    companion object {
        val Start = CarouselAlignment(-1)
        val Center = CarouselAlignment(0)
        val End = CarouselAlignment(1)
    }
}

internal val AnchorSize = 10f // dp value baked by caller before passing to keyline fns
internal const val MediumLargeItemDiffThreshold = 0.85f

internal fun uncontainedKeylineList(
    carouselMainAxisSize: Float,
    itemSize: Float,
    itemSpacing: Float,
    anchorSizePx: Float,
): KeylineList {
    if (carouselMainAxisSize == 0f || itemSize == 0f) {
        return emptyKeylineList()
    }

    val largeItemSize = min(itemSize + itemSpacing, carouselMainAxisSize)
    val largeCount = max(1, floor(carouselMainAxisSize / largeItemSize).toInt())
    val remainingSpace: Float = carouselMainAxisSize - largeCount * largeItemSize

    val mediumCount = if (remainingSpace > 0) 1 else 0
    val mediumItemSize =
        calculateMediumChildSize(
            minimumMediumSize = anchorSizePx,
            largeItemSize = largeItemSize,
            remainingSpace = remainingSpace,
        )
    val arrangement = Arrangement(0, 0F, 0, mediumItemSize, mediumCount, largeItemSize, largeCount)

    val xSmallSize = min(anchorSizePx, itemSize)
    val leftAnchorSize: Float = max(xSmallSize, mediumItemSize * 0.5f)
    return createLeftAlignedKeylineList(
        carouselMainAxisSize = carouselMainAxisSize,
        itemSpacing = itemSpacing,
        leftAnchorSize = leftAnchorSize,
        rightAnchorSize = anchorSizePx,
        arrangement = arrangement,
    )
}

internal fun createLeftAlignedKeylineList(
    carouselMainAxisSize: Float,
    itemSpacing: Float,
    leftAnchorSize: Float,
    rightAnchorSize: Float,
    arrangement: Arrangement,
): KeylineList {
    return keylineListOf(carouselMainAxisSize, itemSpacing, CarouselAlignment.Start) {
        add(leftAnchorSize, isAnchor = true)
        repeat(arrangement.largeCount) { add(arrangement.largeSize) }
        repeat(arrangement.mediumCount) { add(arrangement.mediumSize) }
        repeat(arrangement.smallCount) { add(arrangement.smallSize) }
        add(rightAnchorSize, isAnchor = true)
    }
}

private fun calculateMediumChildSize(
    minimumMediumSize: Float,
    largeItemSize: Float,
    remainingSpace: Float,
): Float {
    var mediumItemSize = minimumMediumSize
    val sizeWithThirdCutOff = remainingSpace * 1.5f
    mediumItemSize = max(sizeWithThirdCutOff, mediumItemSize)

    val largeItemThreshold: Float = largeItemSize * MediumLargeItemDiffThreshold
    if (mediumItemSize > largeItemThreshold) {
        val sizeWithFifthCutOff = remainingSpace * 1.2f
        mediumItemSize = min(max(largeItemThreshold, sizeWithFifthCutOff), largeItemSize)
    }
    return mediumItemSize
}
