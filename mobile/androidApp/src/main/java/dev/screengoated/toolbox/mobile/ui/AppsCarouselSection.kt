@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, ExperimentalTextApi::class, androidx.compose.animation.ExperimentalSharedTransitionApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.carousel.HorizontalUncontainedCarousel
import androidx.compose.material3.carousel.rememberCarouselState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors

private fun appCardTag(index: Int): String = when (index) {
    0 -> "app-card-live-translate"
    1 -> "app-card-translation-gummy"
    2 -> "app-card-video-downloader"
    3 -> "app-card-dj"
    else -> "app-card-placeholder-$index"
}

internal val appSlots = listOf(
    AppSlot(MaterialShapes.Sunny,        { it.appSlotTeal }),   // Live Translate — teal
    AppSlot(MaterialShapes.SemiCircle,   { it.appSlotCoral }),  // placeholder — coral
    AppSlot(MaterialShapes.Heart,        { it.appSlotPurple }), // placeholder — purple
    AppSlot(MaterialShapes.Cookie4Sided, { it.appSlotAmber }),  // placeholder — amber
    AppSlot(MaterialShapes.Clover4Leaf,  { it.appSlotBlue }),   // placeholder — blue
)

@Composable
internal fun AppsCarouselSection(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit = {},
    onDjClick: () -> Unit = {},
    onTranslationGummyClick: () -> Unit = {},
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope? = null,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope? = null,
) {
    val isLandscape = LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    if (isLandscape) {
        AppsHorizontalCarousel(
            state,
            locale,
            onSessionToggle,
            canToggle,
            onDownloaderClick,
            onDjClick,
            onTranslationGummyClick,
            onPagerSwipeLockChanged,
            sharedTransitionScope,
            animatedVisibilityScope,
        )
    } else {
        AppsVerticalCarousel(
            state,
            locale,
            onSessionToggle,
            canToggle,
            onDownloaderClick,
            onDjClick,
            onTranslationGummyClick,
            sharedTransitionScope,
            animatedVisibilityScope,
        )
    }
}

@Composable
private fun AppsItemContent(
    index: Int,
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit,
    onDjClick: () -> Unit,
    onTranslationGummyClick: () -> Unit,
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .testTag(appCardTag(index))
            .then(
                when (index) {
                    1 -> Modifier.clickable(onClick = onTranslationGummyClick)
                    2 -> {
                        val sharedMod = if (sharedTransitionScope != null && animatedVisibilityScope != null) {
                            with(sharedTransitionScope) {
                                Modifier.sharedBounds(
                                    sharedContentState = rememberSharedContentState("downloader-card"),
                                    animatedVisibilityScope = animatedVisibilityScope,
                                    resizeMode = androidx.compose.animation.SharedTransitionScope.ResizeMode.RemeasureToBounds,
                                )
                            }
                        } else Modifier
                        sharedMod.then(Modifier.clickable(onClick = onDownloaderClick))
                    }
                    3 -> Modifier.clickable(onClick = onDjClick)
                    else -> Modifier
                },
            ),
    ) {
        when (index) {
            0 -> LiveTranslateCarouselTile(state = state, locale = locale, onSessionToggle = onSessionToggle, canToggle = canToggle)
            1 -> AppTile(slot = appSlots[1], title = locale.appTranslationGummyTitle, drawableRes = dev.screengoated.toolbox.mobile.R.drawable.ms_breakfast_dining)
            2 -> AppTile(slot = appSlots[2], title = locale.appVideoDownloaderTitle, drawableRes = R.drawable.ms_movie)
            3 -> AppTile(slot = appSlots[3], title = locale.appDjTitle, drawableRes = R.drawable.ms_album)
            else -> EmptyAppTile(slot = appSlots[index])
        }
    }
}

@Composable
private fun AppsVerticalCarousel(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit,
    onDjClick: () -> Unit,
    onTranslationGummyClick: () -> Unit,
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current
    val screenH = with(density) { windowInfo.containerSize.height.toDp() }
    val available = (screenH - 170.dp).coerceAtLeast(320.dp)
    val itemHeight = ((available - 40.dp) / 3.2f).coerceIn(140.dp, 200.dp)
    val carouselHeight = available.coerceAtMost(700.dp)
    val fadeSize = 32.dp
    val bgColor = MaterialTheme.colorScheme.background

    Box(modifier = Modifier.fillMaxWidth().height(carouselHeight)) {
        VerticalUncontainedCarousel(
            itemCount = appSlots.size,
            itemHeight = itemHeight,
            modifier = Modifier.fillMaxWidth().height(carouselHeight),
            itemSpacing = 8.dp,
            contentPadding = PaddingValues(top = 4.dp, bottom = fadeSize),
        ) { index ->
            Box(modifier = Modifier.fillMaxSize().maskClip(MaterialTheme.shapes.extraLarge)) {
                AppsItemContent(
                    index,
                    state,
                    locale,
                    onSessionToggle,
                    canToggle,
                    onDownloaderClick,
                    onDjClick,
                    onTranslationGummyClick,
                    sharedTransitionScope,
                    animatedVisibilityScope,
                )
            }
        }
        Box(modifier = Modifier.fillMaxWidth().height(fadeSize).background(Brush.verticalGradient(listOf(bgColor, Color.Transparent))))
        Box(modifier = Modifier.fillMaxWidth().height(fadeSize).align(Alignment.BottomStart).background(Brush.verticalGradient(listOf(Color.Transparent, bgColor))))
    }
}

@Composable
private fun AppsHorizontalCarousel(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
    onDownloaderClick: () -> Unit,
    onDjClick: () -> Unit,
    onTranslationGummyClick: () -> Unit,
    onPagerSwipeLockChanged: (Boolean) -> Unit,
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current
    val screenH = with(density) { windowInfo.containerSize.height.toDp() }
    val screenW = with(density) { windowInfo.containerSize.width.toDp() }
    val itemWidth = ((screenW - 120.dp) / 3.2f).coerceIn(220.dp, 320.dp)
    val carouselHeight = (screenH - 100.dp).coerceIn(160.dp, 300.dp)
    val fadeSize = 24.dp
    val bgColor = MaterialTheme.colorScheme.background
    val carouselState = rememberCarouselState { appSlots.size }

    Box(modifier = Modifier.fillMaxWidth().height(carouselHeight)) {
        HorizontalUncontainedCarousel(
            state = carouselState,
            itemWidth = itemWidth,
            modifier = Modifier
                .fillMaxSize()
                .lockPagerForCarouselDrag(
                    canScrollBackward = { carouselState.canScrollBackward },
                    canScrollForward = { carouselState.canScrollForward },
                    onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                ),
            itemSpacing = 8.dp,
            contentPadding = PaddingValues(start = 4.dp, end = fadeSize),
        ) { index ->
            Card(
                modifier = Modifier.fillMaxSize().maskClip(MaterialTheme.shapes.extraLarge),
                shape = MaterialTheme.shapes.extraLarge,
                colors = CardDefaults.cardColors(containerColor = appSlots[index].colorToken(MaterialTheme.sgtColors).copy(alpha = 0.15f)),
            ) {
                AppsItemContent(
                    index,
                    state,
                    locale,
                    onSessionToggle,
                    canToggle,
                    onDownloaderClick,
                    onDjClick,
                    onTranslationGummyClick,
                    sharedTransitionScope,
                    animatedVisibilityScope,
                )
            }
        }
        // Left fade
        Box(modifier = Modifier.fillMaxHeight().width(fadeSize).background(Brush.horizontalGradient(listOf(bgColor, Color.Transparent))))
        // Right fade
        Box(modifier = Modifier.fillMaxHeight().width(fadeSize).align(Alignment.CenterEnd).background(Brush.horizontalGradient(listOf(Color.Transparent, bgColor))))
    }
}
