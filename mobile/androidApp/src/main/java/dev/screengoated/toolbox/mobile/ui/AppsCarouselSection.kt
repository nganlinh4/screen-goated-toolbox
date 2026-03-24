@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, ExperimentalTextApi::class, androidx.compose.animation.ExperimentalSharedTransitionApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.carousel.HorizontalUncontainedCarousel
import androidx.compose.material3.carousel.rememberCarouselState
import androidx.compose.material3.toPath
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.SgtExtendedColors
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors

private data class AppSlot(val shape: RoundedPolygon, val colorToken: (SgtExtendedColors) -> Color)

private data class ShapeInstance(
    val shape: RoundedPolygon,
    val xFrac: Float, val yFrac: Float,
    val sizeFrac: Float, val alpha: Float,
    val rotation: Float,
)

private val allDecoShapes by lazy { listOf(
    MaterialShapes.Arch, MaterialShapes.Arrow, MaterialShapes.Boom, MaterialShapes.Bun,
    MaterialShapes.Burst, MaterialShapes.Circle, MaterialShapes.ClamShell,
    MaterialShapes.Clover4Leaf, MaterialShapes.Clover8Leaf,
    MaterialShapes.Cookie12Sided, MaterialShapes.Cookie4Sided, MaterialShapes.Cookie6Sided,
    MaterialShapes.Cookie7Sided, MaterialShapes.Cookie9Sided,
    MaterialShapes.Diamond, MaterialShapes.Fan, MaterialShapes.Flower, MaterialShapes.Gem,
    MaterialShapes.Ghostish, MaterialShapes.Heart, MaterialShapes.Oval, MaterialShapes.Pentagon,
    MaterialShapes.Pill, MaterialShapes.PixelCircle, MaterialShapes.PixelTriangle,
    MaterialShapes.Puffy, MaterialShapes.PuffyDiamond, MaterialShapes.SemiCircle,
    MaterialShapes.Slanted, MaterialShapes.SoftBoom, MaterialShapes.SoftBurst,
    MaterialShapes.Square, MaterialShapes.Sunny, MaterialShapes.Triangle, MaterialShapes.VerySunny,
) }

/** Place shapes with collision detection — no overlapping. */
private fun generateNonOverlappingShapes(seed: Long): List<ShapeInstance> {
    val rng = java.util.Random(seed)
    val placed = mutableListOf<ShapeInstance>()
    var attempts = 0
    while (placed.size < 6 && attempts < 80) {
        attempts++
        val sizeFrac = 0.15f + rng.nextFloat() * 0.75f // tiny to huge
        val xFrac = -0.05f + rng.nextFloat() * 1.10f   // allow overflow left/right
        val yFrac = -0.10f + rng.nextFloat() * 1.20f    // allow overflow top/bottom
        val collides = placed.any { other ->
            val dx = xFrac - other.xFrac
            val dy = yFrac - other.yFrac
            val minDist = (sizeFrac + other.sizeFrac) * 0.32f
            dx * dx + dy * dy < minDist * minDist
        }
        if (!collides) {
            placed.add(ShapeInstance(
                shape = allDecoShapes[rng.nextInt(allDecoShapes.size)],
                xFrac = xFrac, yFrac = yFrac,
                sizeFrac = sizeFrac,
                alpha = 0.10f + rng.nextFloat() * 0.16f,
                rotation = rng.nextFloat() * 360f,
            ))
        }
    }
    return placed
}

/**
 * Non-overlapping shapes with smooth morphing + slight spin + spring bounce.
 * Each shape periodically morphs to another MaterialShape via Morph(A,B).toPath(progress).
 * During the morph, a slight rotation is applied (spring bounce).
 * Idle: morph every 3-6s. Scrolling/active: morph every 0.8-1.6s.
 */
@Composable
private fun AnimatedShapesCanvas(
    color: Color,
    seed: Long,
    isScrolling: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val placements = remember(seed) { generateNonOverlappingShapes(seed) }

    // Per-shape morph state: tracks the current from→to morph pair + generation counter
    // The generation counter drives the animateFloatAsState target flip (0f↔1f)
    data class MorphPair(
        val from: RoundedPolygon,
        val to: RoundedPolygon,
        val gen: Int,
        val spinDelta: Float,
    )

    @Composable
    fun rememberAnimatedShape(i: Int, inst: ShapeInstance): Triple<Morph, Float, Float> {
        var pair by remember { mutableStateOf(MorphPair(inst.shape, inst.shape, 0, 0f)) }

        val intervalMs = if (isScrolling) (800L + i * 200L) else (3000L + i * 1500L)
        LaunchedEffect(isScrolling, i) {
            val rng = java.util.Random(seed + i * 17L)
            while (true) {
                kotlinx.coroutines.delay(intervalMs)
                val nextShape = allDecoShapes[rng.nextInt(allDecoShapes.size)]
                val spinDelta = (rng.nextFloat() - 0.5f) * 30f // ±15° spin during morph
                pair = MorphPair(pair.to, nextShape, pair.gen + 1, spinDelta)
            }
        }

        // Morph progress: animate 0→1 each time gen changes (odd→1, even→0)
        val morphTarget = if (pair.gen % 2 == 0) 0f else 1f
        val morphProgress by animateFloatAsState(
            targetValue = morphTarget,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioNoBouncy,
                stiffness = Spring.StiffnessVeryLow,
            ),
            label = "morph-$i",
        )
        // Actual progress within the current pair: how far from→to
        val t = if (pair.gen % 2 == 0) (1f - morphProgress) else morphProgress

        // Spin: slight rotation during morph (spring bounce)
        val spinTarget = inst.rotation + pair.spinDelta * pair.gen
        val spin by animateFloatAsState(
            targetValue = spinTarget,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioMediumBouncy,
                stiffness = Spring.StiffnessLow,
            ),
            label = "spin-$i",
        )

        val morph = remember(pair.from, pair.to) { Morph(pair.from, pair.to) }
        return Triple(morph, t, spin)
    }

    val animated = placements.mapIndexed { i, inst ->
        val (morph, progress, spin) = rememberAnimatedShape(i, inst)
        Triple(inst, Triple(morph, progress, spin), Unit)
    }

    Canvas(modifier = modifier.fillMaxSize()) {
        animated.forEach { (inst, anim, _) ->
            val (morph, progress, spin) = anim
            val path = morph.toPath(progress = progress)
            val s = size.minDimension * inst.sizeFrac
            if (s < 1f) return@forEach
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s / pathSize
            val matrix = Matrix()
            matrix.translate(size.width * inst.xFrac, size.height * inst.yFrac)
            matrix.rotateZ(spin)
            matrix.scale(scale, scale)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = color.copy(alpha = inst.alpha))
        }
    }
}

private val appSlots = listOf(
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
            onPagerSwipeLockChanged,
            sharedTransitionScope,
            animatedVisibilityScope,
        )
    } else {
        AppsVerticalCarousel(state, locale, onSessionToggle, canToggle, onDownloaderClick, onDjClick, sharedTransitionScope, animatedVisibilityScope)
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
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .then(
                when (index) {
                    1 -> {
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
                    2 -> Modifier.clickable(onClick = onDjClick)
                    else -> Modifier
                },
            ),
    ) {
        when (index) {
            0 -> LiveTranslateCarouselTile(state = state, locale = locale, onSessionToggle = onSessionToggle, canToggle = canToggle)
            1 -> AppTile(slot = appSlots[1], title = locale.appVideoDownloaderTitle, icon = Icons.Rounded.Download)
            2 -> AppTile(slot = appSlots[2], title = locale.appDjTitle, icon = Icons.Rounded.GraphicEq)
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
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    val screenH = LocalConfiguration.current.screenHeightDp.dp
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
                AppsItemContent(index, state, locale, onSessionToggle, canToggle, onDownloaderClick, onDjClick, sharedTransitionScope, animatedVisibilityScope)
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
    onPagerSwipeLockChanged: (Boolean) -> Unit,
    sharedTransitionScope: androidx.compose.animation.SharedTransitionScope?,
    animatedVisibilityScope: androidx.compose.animation.AnimatedVisibilityScope?,
) {
    val screenH = LocalConfiguration.current.screenHeightDp.dp
    val screenW = LocalConfiguration.current.screenWidthDp.dp
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
                AppsItemContent(index, state, locale, onSessionToggle, canToggle, onDownloaderClick, onDjClick, sharedTransitionScope, animatedVisibilityScope)
            }
        }
        // Left fade
        Box(modifier = Modifier.fillMaxHeight().width(fadeSize).background(Brush.horizontalGradient(listOf(bgColor, Color.Transparent))))
        // Right fade
        Box(modifier = Modifier.fillMaxHeight().width(fadeSize).align(Alignment.CenterEnd).background(Brush.horizontalGradient(listOf(Color.Transparent, bgColor))))
    }
}

@Composable
private fun LiveTranslateCarouselTile(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
) {
    val isRunning = state.phase in setOf(
        SessionPhase.STARTING, SessionPhase.LISTENING, SessionPhase.TRANSLATING,
    )
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val slot = appSlots[0]
    val slotColor = slot.colorToken(MaterialTheme.sgtColors)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(if (isRunning) slotColor.copy(alpha = 0.30f) else slotColor.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slotColor,
            seed = slotColor.hashCode().toLong() xor 0x42L,
            isScrolling = isRunning, // morph faster when live translate is active
        )
        val stretchedFamily = remember {
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
                FontFamily(
                    androidx.compose.ui.text.font.Font(
                        resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                        weight = FontWeight.Black,
                        variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                            androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Black.weight),
                            androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                            androidx.compose.ui.text.font.FontVariation.Setting("wdth", 125f),
                        ),
                    ),
                )
            } else {
                FontFamily.Default
            }
        }
        if (isLandscape) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Icon(
                        Icons.Rounded.Translate,
                        contentDescription = null,
                        tint = slotColor,
                        modifier = Modifier.size(40.dp),
                    )
                    Text(
                        text = locale.shellLiveTitle,
                        fontFamily = stretchedFamily,
                        fontWeight = FontWeight.Black,
                        fontSize = 22.sp,
                        lineHeight = 24.sp,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 3,
                        modifier = Modifier.weight(1f),
                    )
                }
                Button(
                    onClick = onSessionToggle,
                    enabled = canToggle,
                    shape = CircleShape,
                    colors = if (isRunning) {
                        ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                    } else {
                        ButtonDefaults.buttonColors()
                    },
                    modifier = Modifier.align(Alignment.End),
                ) {
                    Icon(
                        if (isRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(ButtonDefaults.IconSpacing))
                    Text(if (isRunning) locale.turnOff else locale.turnOn)
                }
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    Icons.Rounded.Translate,
                    contentDescription = null,
                    tint = slotColor,
                    modifier = Modifier.size(44.dp),
                )
                Spacer(Modifier.width(14.dp))
                Column(modifier = Modifier.weight(1f)) {
                    val words = locale.shellLiveTitle.split(" ", limit = 2)
                    if (words.isNotEmpty()) {
                        Text(
                            text = words[0],
                            fontFamily = stretchedFamily,
                            fontWeight = FontWeight.Black,
                            fontSize = 28.sp,
                            lineHeight = 32.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    if (words.size > 1) {
                        Text(
                            text = words[1],
                            fontWeight = FontWeight.Bold,
                            fontSize = 26.sp,
                            lineHeight = 30.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
                Column(
                    horizontalAlignment = Alignment.End,
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Button(
                        onClick = onSessionToggle,
                        enabled = canToggle,
                        shape = CircleShape,
                        colors = if (isRunning) {
                            ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                        } else {
                            ButtonDefaults.buttonColors()
                        },
                    ) {
                        Icon(
                            if (isRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(ButtonDefaults.IconSpacing))
                        Text(if (isRunning) locale.turnOff else locale.turnOn)
                    }
                }
            }
        }
    }
}

@Composable
private fun AppTile(
    slot: AppSlot,
    title: String,
    icon: ImageVector?,
) {
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val slotColor = slot.colorToken(MaterialTheme.sgtColors)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(slotColor.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slotColor,
            seed = slotColor.hashCode().toLong(),
        )
        val stretchedFamily = remember {
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
                FontFamily(
                    androidx.compose.ui.text.font.Font(
                        resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                        weight = FontWeight.Black,
                        variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                            androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Black.weight),
                            androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                            androidx.compose.ui.text.font.FontVariation.Setting("wdth", 125f),
                        ),
                    ),
                )
            } else {
                FontFamily.Default
            }
        }
        if (isLandscape) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                if (icon != null) {
                    Icon(
                        icon,
                        contentDescription = null,
                        tint = slotColor,
                        modifier = Modifier.size(40.dp),
                    )
                }
                Text(
                    text = title,
                    fontFamily = stretchedFamily,
                    fontWeight = FontWeight.Black,
                    fontSize = 22.sp,
                    lineHeight = 24.sp,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 3,
                )
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (icon != null) {
                    Icon(
                        icon,
                        contentDescription = null,
                        tint = slotColor,
                        modifier = Modifier.size(44.dp),
                    )
                    Spacer(Modifier.width(14.dp))
                }
                Column(modifier = Modifier.weight(1f)) {
                    val words = title.split(" ", limit = 2)
                    if (words.isNotEmpty()) {
                        Text(
                            text = words[0],
                            fontFamily = stretchedFamily,
                            fontWeight = FontWeight.Black,
                            fontSize = 28.sp,
                            lineHeight = 32.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    if (words.size > 1) {
                        Text(
                            text = words[1],
                            fontWeight = FontWeight.Bold,
                            fontSize = 26.sp,
                            lineHeight = 30.sp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyAppTile(slot: AppSlot) {
    val slotColor = slot.colorToken(MaterialTheme.sgtColors)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(slotColor.copy(alpha = 0.15f)),
    ) {
        AnimatedShapesCanvas(
            color = slotColor,
            seed = slotColor.hashCode().toLong(),
        )
    }
}
