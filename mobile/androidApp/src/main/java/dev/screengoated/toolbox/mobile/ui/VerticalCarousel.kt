/*
 * Vertical carousel — ported from M3E's internal Carousel(orientation=Vertical).
 * Strategy/Keyline machinery adapted from androidx.compose.material3.carousel.*
 * Original Copyright 2024 The Android Open Source Project, Apache 2.0
 */
package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.gestures.snapping.SnapPosition
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.pager.PageSize
import androidx.compose.foundation.pager.PagerDefaults
import androidx.compose.foundation.pager.PagerSnapDistance
import androidx.compose.foundation.pager.PagerState
import androidx.compose.foundation.pager.VerticalPager
import androidx.compose.foundation.shape.GenericShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.toRect
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.ui.carousel.KeylineList
import dev.screengoated.toolbox.mobile.ui.carousel.Strategy
import dev.screengoated.toolbox.mobile.ui.carousel.lerp
import dev.screengoated.toolbox.mobile.ui.carousel.uncontainedKeylineList
import kotlin.math.roundToInt

// ---------------------------------------------------------------------------
// Public / internal API
// ---------------------------------------------------------------------------

/**
 * Vertical equivalent of Material3 Expressive's [HorizontalUncontainedCarousel].
 *
 * Ported from M3E's internal `Carousel(orientation = Orientation.Vertical)` + the full
 * Strategy/Keyline machinery. Items are sized by the carousel's keyline strategy: focal
 * items occupy [itemHeight], and the single peek item at the viewport edge is sized to the
 * remaining space and shrinks/grows as it scrolls through the focal range — exactly matching
 * the M3E horizontal behaviour.
 *
 * Content receives a [VerticalCarouselItemScope] receiver. Call
 * [VerticalCarouselItemScope.maskClip] on the item's root composable to enable the mask-clip
 * visual (rounded corners follow the visible portion of the item, identical to M3E's
 * [androidx.compose.material3.carousel.CarouselItemScope.maskClip]).
 */
@Composable
internal fun VerticalUncontainedCarousel(
    itemCount: Int,
    itemHeight: Dp,
    modifier: Modifier = Modifier,
    itemSpacing: Dp = 0.dp,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    content: @Composable VerticalCarouselItemScope.(index: Int) -> Unit,
) {
    val density = LocalDensity.current
    val beforeContentPaddingPx = with(density) { contentPadding.calculateTopPadding().toPx() }
    val afterContentPaddingPx = with(density) { contentPadding.calculateBottomPadding().toPx() }
    val anchorSizePx = with(density) { VcAnchorSize.toPx() }

    val pageSize = remember(itemHeight, itemSpacing, beforeContentPaddingPx, afterContentPaddingPx, anchorSizePx) {
        VcCarouselPageSize(
            itemSizeDp = itemHeight,
            anchorSizePx = anchorSizePx,
            itemSpacingDp = itemSpacing,
            beforeContentPadding = beforeContentPaddingPx,
            afterContentPadding = afterContentPaddingPx,
            density = density,
        )
    }

    val pagerState = rememberSaveable(saver = VcPagerState.Saver) {
        VcPagerState(currentPage = 0, itemCount = itemCount)
    }.also { it.pageCountLambda = { itemCount } }

    val snapPosition = remember(pageSize) { VcKeylineSnapPosition(pageSize) }

    val flingBehavior = PagerDefaults.flingBehavior(
        state = pagerState,
        pagerSnapDistance = PagerSnapDistance.atMost(itemCount),
    )

    // Prevent the outer verticalScroll from eating fling velocity that the pager needs for
    // snap animations. Post-scroll/fling velocity consumed here rather than propagated up.
    val nestedScrollBlocker = remember {
        object : NestedScrollConnection {
            override fun onPostScroll(
                consumed: Offset, available: Offset, source: NestedScrollSource,
            ): Offset = available
            override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity =
                available
        }
    }

    VerticalPager(
        state = pagerState,
        contentPadding = contentPadding,
        pageSize = pageSize,
        pageSpacing = itemSpacing,
        beyondViewportPageCount = 1,
        snapPosition = snapPosition,
        flingBehavior = flingBehavior,
        modifier = modifier.nestedScroll(nestedScrollBlocker).semantics { role = Role.Carousel },
    ) { page ->
        val itemDrawInfo = remember { VcItemDrawInfo() }
        val scope = remember(itemDrawInfo) { VcItemScopeImpl(itemDrawInfo) }
        val clipShape = remember {
            object : Shape {
                override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density) =
                    androidx.compose.ui.graphics.Outline.Rectangle(itemDrawInfo.maskRect)
            }
        }
        Box(
            modifier = Modifier.vcCarouselItem(
                index = page,
                pagerState = pagerState,
                strategy = { pageSize.strategy },
                itemDrawInfo = itemDrawInfo,
                clipShape = clipShape,
            )
        ) {
            scope.content(page)
        }
    }
}

/**
 * Receiver scope for items inside a [VerticalUncontainedCarousel].
 *
 * Mirrors [androidx.compose.material3.carousel.CarouselItemScope]:
 * call [maskClip] on the item's root composable to enable M3E-style rounded mask clipping.
 */
internal sealed interface VerticalCarouselItemScope {
    @Composable
    fun Modifier.maskClip(shape: Shape): Modifier

    @Composable
    fun rememberMaskShape(shape: Shape): GenericShape
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

private val VcAnchorSize = 10.dp

/** Mutable draw info updated each frame in [vcCarouselItem]. */
private class VcItemDrawInfo {
    var maskRectState by mutableStateOf(Rect.Zero)
    val maskRect: Rect get() = maskRectState
}

private class VcItemScopeImpl(private val info: VcItemDrawInfo) : VerticalCarouselItemScope {
    @Composable
    override fun rememberMaskShape(shape: Shape): GenericShape {
        val density = LocalDensity.current
        return remember(info.maskRect, density) {
            GenericShape { size, direction ->
                val rect = info.maskRect.intersect(size.toRect())
                addOutline(shape.createOutline(rect.size, direction, density))
                translate(Offset(rect.left, rect.top))
            }
        }
    }

    @Composable
    override fun Modifier.maskClip(shape: Shape): Modifier =
        clip(rememberMaskShape(shape))
}

/** Custom [PagerState] with a mutable item-count lambda, mirroring [CarouselPagerState]. */
private class VcPagerState(
    currentPage: Int,
    itemCount: Int,
) : PagerState(currentPage, 0f) {
    var pageCountLambda: () -> Int = { itemCount }
    override val pageCount: Int get() = pageCountLambda()

    companion object {
        val Saver = androidx.compose.runtime.saveable.listSaver<VcPagerState, Any>(
            save = { listOf(it.currentPage, it.pageCount) },
            restore = { VcPagerState(it[0] as Int, it[1] as Int) },
        )
    }
}

/** [PageSize] implementation that keeps the [Strategy] updated with the latest available space. */
private class VcCarouselPageSize(
    private val itemSizeDp: Dp,
    private val anchorSizePx: Float,
    private val itemSpacingDp: Dp,
    private val beforeContentPadding: Float,
    private val afterContentPadding: Float,
    private val density: Density,
) : PageSize {
    private var strategyState by mutableStateOf(Strategy.Empty)
    val strategy: Strategy get() = strategyState

    override fun Density.calculateMainAxisPageSize(availableSpace: Int, pageSpacing: Int): Int {
        val itemSizePx = with(density) { itemSizeDp.toPx() }
        val keylines = uncontainedKeylineList(
            carouselMainAxisSize = availableSpace.toFloat(),
            itemSize = itemSizePx,
            itemSpacing = pageSpacing.toFloat(),
            anchorSizePx = anchorSizePx,
        )
        strategyState = Strategy(
            keylines,
            availableSpace.toFloat(),
            pageSpacing.toFloat(),
            beforeContentPadding,
            afterContentPadding,
        )
        return if (strategy.isValid) strategy.itemMainAxisSize.roundToInt() else availableSpace
    }
}

/** Applies size-from-keylines, mask clipping, and translation to a carousel item. Vertical-only. */
private fun Modifier.vcCarouselItem(
    index: Int,
    pagerState: VcPagerState,
    strategy: () -> Strategy,
    itemDrawInfo: VcItemDrawInfo,
    clipShape: Shape,
): Modifier = layout { measurable, constraints ->
    val strategyResult = strategy()
    if (!strategyResult.isValid) {
        return@layout layout(0, 0) {}
    }

    val mainAxisSize = strategyResult.itemMainAxisSize
    val itemConstraints = constraints.copy(
        minHeight = mainAxisSize.roundToInt(),
        maxHeight = mainAxisSize.roundToInt(),
    )
    val placeable = measurable.measure(itemConstraints)

    val itemZIndex = if (index == pagerState.currentPage) 1f
        else if (index == 0) 0f
        else 1f / index.toFloat()

    layout(placeable.width, placeable.height) {
        placeable.placeWithLayer(0, 0, zIndex = itemZIndex) {
            val scrollOffset = vcCalculateCurrentScrollOffset(pagerState, strategyResult)
            val maxScrollOffset = vcCalculateMaxScrollOffset(pagerState, strategyResult)
            val keylines = strategyResult.getKeylineListForScrollOffset(scrollOffset, maxScrollOffset)
            val roundedKeylines = strategyResult.getKeylineListForScrollOffset(
                scrollOffset, maxScrollOffset, roundToNearestStep = true
            )

            val itemSizeWithSpacing = strategyResult.itemMainAxisSize + strategyResult.itemSpacing
            val unadjustedCenter =
                (index * itemSizeWithSpacing) + (strategyResult.itemMainAxisSize / 2f) - scrollOffset

            val keylineBefore = keylines.getKeylineBefore(unadjustedCenter)
            val keylineAfter = keylines.getKeylineAfter(unadjustedCenter)
            val progress = vcGetProgress(keylineBefore, keylineAfter, unadjustedCenter)
            val interpolatedKeyline = lerp(keylineBefore, keylineAfter, progress)
            val isOutOfKeylineBounds = keylineBefore == keylineAfter

            // Vertical: mask clips height, full width preserved
            val centerY = strategyResult.itemMainAxisSize / 2f
            val halfMaskHeight = interpolatedKeyline.size / 2f
            val halfMaskWidth = size.width / 2f
            val maskRect = Rect(
                left = size.width / 2f - halfMaskWidth,
                top = centerY - halfMaskHeight,
                right = size.width / 2f + halfMaskWidth,
                bottom = centerY + halfMaskHeight,
            )

            itemDrawInfo.maskRectState = maskRect

            clip = maskRect != Rect(0f, 0f, size.width, size.height)
            shape = clipShape

            var translation = interpolatedKeyline.offset - unadjustedCenter
            if (isOutOfKeylineBounds) {
                val outOfBoundsOffset =
                    (unadjustedCenter - interpolatedKeyline.unadjustedOffset) / interpolatedKeyline.size
                translation += outOfBoundsOffset
            }
            translationY = translation
        }
    }
}

private fun vcCalculateCurrentScrollOffset(pagerState: VcPagerState, strategy: Strategy): Float {
    val itemSizeWithSpacing = strategy.itemMainAxisSize + strategy.itemSpacing
    val currentItemScrollOffset =
        (pagerState.currentPage * itemSizeWithSpacing) +
            (pagerState.currentPageOffsetFraction * itemSizeWithSpacing)
    return currentItemScrollOffset - vcGetSnapPositionOffset(strategy, pagerState.currentPage, pagerState.pageCount)
}

private fun vcCalculateMaxScrollOffset(pagerState: VcPagerState, strategy: Strategy): Float {
    val itemCount = pagerState.pageCount.toFloat()
    val maxScrollPossible =
        (strategy.itemMainAxisSize * itemCount) + (strategy.itemSpacing * (itemCount - 1))
    return (maxScrollPossible - strategy.availableSpace).coerceAtLeast(0f)
}

private fun vcGetProgress(
    before: dev.screengoated.toolbox.mobile.ui.carousel.Keyline,
    after: dev.screengoated.toolbox.mobile.ui.carousel.Keyline,
    unadjustedOffset: Float,
): Float {
    if (before == after) return 1f
    val total = after.unadjustedOffset - before.unadjustedOffset
    return (unadjustedOffset - before.unadjustedOffset) / total
}

private fun vcGetSnapPositionOffset(strategy: Strategy, itemIndex: Int, itemCount: Int): Int {
    if (!strategy.isValid) return 0
    var offset =
        (strategy.defaultKeylines.firstFocal.unadjustedOffset - strategy.itemMainAxisSize / 2f)
            .roundToInt()
    if (itemIndex <= strategy.startKeylineSteps.lastIndex) {
        val stepIndex = (strategy.startKeylineSteps.lastIndex - itemIndex)
            .coerceIn(0, strategy.startKeylineSteps.lastIndex)
        val startKeylines = strategy.startKeylineSteps[stepIndex]
        offset = (startKeylines.firstFocal.unadjustedOffset - strategy.itemMainAxisSize / 2f).roundToInt()
    }
    val lastItemIndex = itemCount - 1
    if (itemIndex >= lastItemIndex - strategy.endKeylineSteps.lastIndex &&
        itemCount > strategy.defaultKeylines.focalCount) {
        val stepIndex = (strategy.endKeylineSteps.lastIndex - (lastItemIndex - itemIndex))
            .coerceIn(0, strategy.endKeylineSteps.lastIndex)
        val endKeylines = strategy.endKeylineSteps[stepIndex]
        offset = (endKeylines.lastFocal.unadjustedOffset - strategy.itemMainAxisSize / 2f).roundToInt()
    }
    return offset
}

private fun VcKeylineSnapPosition(pageSize: VcCarouselPageSize): SnapPosition =
    object : SnapPosition {
        override fun position(
            layoutSize: Int,
            itemSize: Int,
            beforeContentPadding: Int,
            afterContentPadding: Int,
            itemIndex: Int,
            itemCount: Int,
        ): Int = vcGetSnapPositionOffset(pageSize.strategy, itemIndex, itemCount)
    }
